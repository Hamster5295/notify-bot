use actix_web::{
    post, web::{Data, Path}, HttpRequest, HttpResponse, Responder
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use reqwest::Client;
use serde_json::{json, Value};
use tklog::{info, warn};

use crate::config::{NotifyConfig, RuntimeConfig};

#[post("/notify-{id}")]
pub async fn notify(
    req: HttpRequest,
    auth: Option<BearerAuth>,
    id: Path<String>,
    body: String,
    conf: Data<RuntimeConfig>,
    client: Data<Client>,
) -> impl Responder {
    let id = id.into_inner();

    if let Some(addr) = req.peer_addr() {
        info!(format!("{} - /notify-{}", addr, id));
    } else {
        info!(format!("Unknown Addr - /notify-{}", id));
    }

    if let Some(cfg) = conf.notifications.get(&id) {
        if cfg.token.is_none() || cfg.token == auth.as_ref().map(|a| a.token().to_string()) {
            handle_notify_request(&body, cfg, &conf, &client).await;
        } else {
            if auth.is_none() {
                warn!("No token provided. Rejected.")
            } else {
                warn!(format!("Wrone token provided. Rejected."));
            }
            return HttpResponse::Unauthorized().body("Permission Denied.");
        }
    } else {
        warn!(format!(
            "No config found with the corresponding ID [{}]",
            id
        ));
        return HttpResponse::NotFound().body("The requested notify ID is not registered.");
    }
    HttpResponse::Ok().finish()
}

async fn handle_notify_request(
    req: &String,
    notify_cfg: &NotifyConfig,
    runtime_cfg: &RuntimeConfig,
    client: &Client,
) {
    let mut msg = notify_cfg.message.clone();

    if notify_cfg.extra.unwrap_or(false) {
        let mut extra: Option<String> = None;
        let mut has_error = false;

        if let Some(extract) = &notify_cfg.extractor {
            let paths: Vec<&str> = extract.split('.').collect();
            let val: Result<Value, serde_json::Error> = serde_json::from_str(&req);
            if let Err(err) = val {
                warn!(format!("Failed to parse body as json: {}", err));
                has_error = true;
            } else {
                let mut val = val.unwrap();
                let mut idx = 0;
                for path in paths {
                    idx = idx + path.len() + 1;
                    match path {
                        "$" => {
                            let res: Result<Value, serde_json::Error> =
                                serde_json::from_str(val.as_str().unwrap_or_default());
                            if let Err(err) = res {
                                warn!(format!("Failed to parse body as json: {}", err));
                                has_error = true;
                                break;
                            } else {
                                val = res.unwrap();
                            }
                        }
                        _ => {
                            if let Some(field) = val.get(path) {
                                val = field.clone();
                            } else {
                                warn!(format!(
                                    "Cannot find the specified extract path: {}",
                                    &extract[..idx-1]
                                ));
                                has_error = true;
                            }
                        }
                    }
                }
                extra = val.as_str().map(|s| s.to_string());
            }
        } else {
            extra = Some(req.to_owned());
        }

        if has_error {
            extra = None;
            warn!(format!("Received body: \n\n{}\n", req));
        } 
         if let Some(extra) = extra.or(notify_cfg.extract_fallback.clone()) {
            msg = format!("{}\n{}", msg, extra).trim().to_string();
        }
    }
    info!(format!("Sending Message: \n\n{}\n", msg));

    if let Some(people) = &notify_cfg.people {
        let send_private_url = format!("{}/send_private_msg", runtime_cfg.onebot.url);
        for person in people {
            tokio::spawn(
                client
                    .post(&send_private_url)
                    .body(
                        json!({
                            "user_id": person,
                            "message": [
                                {
                                    "type": "text",
                                    "data": {
                                        "text": msg
                                    }
                                }
                            ]
                        })
                        .to_string(),
                    )
                    .send(),
            );
        }
    }

    if let Some(groups) = &notify_cfg.groups {
        let send_group_url = format!("{}/send_group_msg", runtime_cfg.onebot.url);

        if let Some(mentions) = &notify_cfg.mentions {
            msg = format!("{}\n", msg);
            for mention in mentions {
                msg = format!("{} [CQ:at,qq={}]", msg, mention);
            }
        }

        for group in groups {
            tokio::spawn(
                client
                    .post(&send_group_url)
                    .body(
                        json!({
                            "group_id": group,
                            "message": [
                                {
                                    "type": "text",
                                    "data": {
                                        "text": msg
                                    }
                                }
                            ]
                        })
                        .to_string(),
                    )
                    .send(),
            );
        }
    }

    info!("Notification Sent!");
}

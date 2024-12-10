use std::{collections::HashMap, vec};

use actix_web::{
    post,
    web::{Data, Path},
    HttpRequest, HttpResponse, Responder,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use reqwest::Client;
use serde_json::{json, Value};
use strfmt::strfmt;
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
            info!("Handling request body: \n{}", body);
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
        if notify_cfg.extractors.is_some() && notify_cfg.extractors.as_ref().unwrap().len() > 0 {
            let mut contents: HashMap<String, String> = HashMap::new();

            for extract in notify_cfg.extractors.as_ref().unwrap() {
                let val: Result<Value, serde_json::Error> = serde_json::from_str(&req);
                if let Err(err) = val {
                    warn!(format!("Failed to parse body as json: {}", err));
                } else {
                    let sep = extract.sep.clone().unwrap_or(" ".to_string());

                    if let Some(res) =
                        extract_arg(&val.unwrap(), &extract.path, &sep).or(extract.fallback.clone())
                    {
                        contents.insert(extract.name.clone(), res);
                    }
                }
            }

            if let Ok(res) = strfmt(msg.as_str(), &contents) {
                msg = res;
            } else {
                warn!("Failed to format message with extracted contents.")
            }
        } else {
            warn!("Extra is set to true, but no extractors are defined.")
        }
    }
    info!(format!("Sending Message: \n\n{}\n", msg));

    if let Some(user) = &notify_cfg.users {
        let send_private_url = format!("{}/send_private_msg", runtime_cfg.onebot.url);
        for person in user {
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

fn extract_arg(val: &Value, path: &String, sep: &str) -> Option<String> {
    extract_arg_impl(val.clone(), &path.split('.').collect(), sep, 0)
}

fn extract_arg_impl(val: Value, paths: &Vec<&str>, sep: &str, idx: usize) -> Option<String> {
    if idx >= paths.len() {
        return val.as_str().map(|s| s.to_string());
    }

    let current_path = paths[idx];
    let mut val = val;

    if current_path.starts_with('[') && current_path.ends_with(']') {
        if !val.is_array() {
            warn!(format!(
                "The value at path: {} is not an array",
                paths[..=idx].join(".")
            ));
            return None;
        }
        let arr = val.as_array().unwrap();

        let idxs = current_path[1..current_path.len() - 1].to_string();
        let mut range = vec![];

        if idxs.len() == 0 {
            for i in 0..arr.len() {
                range.push(i);
            }
        } else {
            let idxs: Vec<&str> = idxs.split(',').collect();
            for i in idxs {
                if let Ok(i) = i.parse() {
                    range.push(i);
                } else {
                    warn!(format!(
                        "Unrecognized Index: {} at {}",
                        i,
                        paths[..=idx].join(".")
                    ));
                    return None;
                }
            }
        }

        let mut results = vec![];
        for i in range {
            let res = extract_arg_impl(arr[i].clone(), paths, sep, idx + 1);
            if let Some(res) = res {
                results.push(res);
            } else {
                warn!(format!("The error occurred at index \x1b[1m{}\x1b[0m of the array", i));
            }
        }

        return Some(results.join(sep));
    }

    match current_path {
        "$" => {
            let res: Result<Value, serde_json::Error> =
                serde_json::from_str(val.as_str().unwrap_or_default());
            if let Err(err) = res {
                warn!(format!("Failed to parse body as json: {}", err));
                return None;
            } else {
                val = res.unwrap();
            }
        }
        _ => {
            if let Some(field) = val.get(current_path) {
                val = field.clone();
            } else {
                warn!(format!(
                    "Cannot find the specified extract path: {}",
                    paths[..=idx].join(".")
                ));
                return None;
            }
        }
    }
    extract_arg_impl(val, paths, sep, idx + 1)
}

#[cfg(test)]
mod tests {
    use super::extract_arg;
    use serde_json::json;

    #[test]
    fn test_extract_arg() {
        let val = json!(
            {
                "simple": "1",
                "nesting":{
                    "so":{
                        "deep":"2"
                    }
                },
                "list": [
                    "3",
                    "4",
                    "5"
                ],
                "nesting-list": [
                    {
                        "nested": "6",
                        "special": "8"
                    },
                    {
                        "nested": "7"
                    }
                ]
            }
        );

        let sep = ",";
        assert_eq!(extract_arg(&val, &"simple".to_string(), sep).unwrap(), "1");
        assert_eq!(
            extract_arg(&val, &"nesting.so.deep".to_string(), sep).unwrap(),
            "2"
        );
        assert_eq!(
            extract_arg(&val, &"list.[]".to_string(), sep).unwrap(),
            "3,4,5"
        );
        assert_eq!(
            extract_arg(&val, &"list.[1,2]".to_string(), sep).unwrap(),
            "4,5"
        );
        assert_eq!(
            extract_arg(&val, &"nesting-list.[].nested".to_string(), sep).unwrap(),
            "6,7"
        );
        assert_eq!(
            extract_arg(&val, &"nesting-list.[].special".to_string(), sep).unwrap(),
            "8"
        );
    }
}

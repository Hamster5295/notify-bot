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
    let mut msg: String = notify_cfg.message.clone();

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
            warn!("EXTRA is set to true, but no extractors are defined.")
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
                warn!(format!(
                    "The error occurred at index \x1b[1m{}\x1b[0m of the array",
                    i
                ));
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

    #[test]
    fn test_github_webhook() {
        let val = json!({
          "ref": "refs/heads/main",
          "before": "1e84324b2c4788b684e53598161ffdcc17d4146d",
          "after": "671d0e94215730b2d294e280ce2f03996b213323",
          "repository": {
            "id": 897667274,
            "node_id": "R_kgDONYFQyg",
            "name": "digital-ic-proj",
            "full_name": "Hamster5295/digital-ic-proj",
            "private": true,
            "owner": {
              "name": "Hamster5295",
              "email": "37259613+Hamster5295@users.noreply.github.com",
              "login": "Hamster5295",
              "id": 37259613,
              "node_id": "MDQ6VXNlcjM3MjU5NjEz",
              "avatar_url": "https://avatars.githubusercontent.com/u/37259613?v=4",
              "gravatar_id": "",
              "url": "https://api.github.com/users/Hamster5295",
              "html_url": "https://github.com/Hamster5295",
              "followers_url": "https://api.github.com/users/Hamster5295/followers",
              "following_url": "https://api.github.com/users/Hamster5295/following{/other_user}",
              "gists_url": "https://api.github.com/users/Hamster5295/gists{/gist_id}",
              "starred_url": "https://api.github.com/users/Hamster5295/starred{/owner}{/repo}",
              "subscriptions_url": "https://api.github.com/users/Hamster5295/subscriptions",
              "organizations_url": "https://api.github.com/users/Hamster5295/orgs",
              "repos_url": "https://api.github.com/users/Hamster5295/repos",
              "events_url": "https://api.github.com/users/Hamster5295/events{/privacy}",
              "received_events_url": "https://api.github.com/users/Hamster5295/received_events",
              "type": "User",
              "user_view_type": "public",
              "site_admin": false
            },
            "html_url": "https://github.com/Hamster5295/digital-ic-proj",
            "description": "数集课设",
            "fork": false,
            "url": "https://github.com/Hamster5295/digital-ic-proj",
            "forks_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/forks",
            "keys_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/keys{/key_id}",
            "collaborators_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/collaborators{/collaborator}",
            "teams_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/teams",
            "hooks_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/hooks",
            "issue_events_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/issues/events{/number}",
            "events_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/events",
            "assignees_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/assignees{/user}",
            "branches_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/branches{/branch}",
            "tags_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/tags",
            "blobs_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/git/blobs{/sha}",
            "git_tags_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/git/tags{/sha}",
            "git_refs_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/git/refs{/sha}",
            "trees_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/git/trees{/sha}",
            "statuses_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/statuses/{sha}",
            "languages_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/languages",
            "stargazers_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/stargazers",
            "contributors_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/contributors",
            "subscribers_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/subscribers",
            "subscription_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/subscription",
            "commits_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/commits{/sha}",
            "git_commits_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/git/commits{/sha}",
            "comments_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/comments{/number}",
            "issue_comment_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/issues/comments{/number}",
            "contents_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/contents/{+path}",
            "compare_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/compare/{base}...{head}",
            "merges_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/merges",
            "archive_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/{archive_format}{/ref}",
            "downloads_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/downloads",
            "issues_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/issues{/number}",
            "pulls_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/pulls{/number}",
            "milestones_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/milestones{/number}",
            "notifications_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/notifications{?since,all,participating}",
            "labels_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/labels{/name}",
            "releases_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/releases{/id}",
            "deployments_url": "https://api.github.com/repos/Hamster5295/digital-ic-proj/deployments",
            "created_at": 1733194602,
            "updated_at": "2024-12-10T06:04:19Z",
            "pushed_at": 1733811315,
            "git_url": "git://github.com/Hamster5295/digital-ic-proj.git",
            "ssh_url": "git@github.com:Hamster5295/digital-ic-proj.git",
            "clone_url": "https://github.com/Hamster5295/digital-ic-proj.git",
            "svn_url": "https://github.com/Hamster5295/digital-ic-proj",
            "homepage": null,
            "size": 7,
            "stargazers_count": 0,
            "watchers_count": 0,
            "language": "Verilog",
            "has_issues": true,
            "has_projects": true,
            "has_downloads": true,
            "has_wiki": false,
            "has_pages": false,
            "has_discussions": false,
            "forks_count": 0,
            "mirror_url": null,
            "archived": false,
            "disabled": false,
            "open_issues_count": 0,
            "license": null,
            "allow_forking": true,
            "is_template": false,
            "web_commit_signoff_required": false,
            "topics": [

            ],
            "visibility": "private",
            "forks": 0,
            "open_issues": 0,
            "watchers": 0,
            "default_branch": "main",
            "stargazers": 0,
            "master_branch": "main"
          },
          "pusher": {
            "name": "Hamster5295",
            "email": "37259613+Hamster5295@users.noreply.github.com"
          },
          "sender": {
            "login": "Hamster5295",
            "id": 37259613,
            "node_id": "MDQ6VXNlcjM3MjU5NjEz",
            "avatar_url": "https://avatars.githubusercontent.com/u/37259613?v=4",
            "gravatar_id": "",
            "url": "https://api.github.com/users/Hamster5295",
            "html_url": "https://github.com/Hamster5295",
            "followers_url": "https://api.github.com/users/Hamster5295/followers",
            "following_url": "https://api.github.com/users/Hamster5295/following{/other_user}",
            "gists_url": "https://api.github.com/users/Hamster5295/gists{/gist_id}",
            "starred_url": "https://api.github.com/users/Hamster5295/starred{/owner}{/repo}",
            "subscriptions_url": "https://api.github.com/users/Hamster5295/subscriptions",
            "organizations_url": "https://api.github.com/users/Hamster5295/orgs",
            "repos_url": "https://api.github.com/users/Hamster5295/repos",
            "events_url": "https://api.github.com/users/Hamster5295/events{/privacy}",
            "received_events_url": "https://api.github.com/users/Hamster5295/received_events",
            "type": "User",
            "user_view_type": "public",
            "site_admin": false
          },
          "created": false,
          "deleted": false,
          "forced": false,
          "base_ref": null,
          "compare": "https://github.com/Hamster5295/digital-ic-proj/compare/1e84324b2c47...671d0e942157",
          "commits": [
            {
              "id": "671d0e94215730b2d294e280ce2f03996b213323",
              "tree_id": "2f14d338b38da4b4169810fa0a0c455ad35b9a11",
              "distinct": true,
              "message": "chore: 测试 bot",
              "timestamp": "2024-12-10T14:15:11+08:00",
              "url": "https://github.com/Hamster5295/digital-ic-proj/commit/671d0e94215730b2d294e280ce2f03996b213323",
              "author": {
                "name": "Hamster5295",
                "email": "hamster5295@163.com",
                "username": "Hamster5295"
              },
              "committer": {
                "name": "Hamster5295",
                "email": "hamster5295@163.com",
                "username": "Hamster5295"
              },
              "added": [

              ],
              "removed": [

              ],
              "modified": [
                "FSM.v"
              ]
            }
          ],
          "head_commit": {
            "id": "671d0e94215730b2d294e280ce2f03996b213323",
            "tree_id": "2f14d338b38da4b4169810fa0a0c455ad35b9a11",
            "distinct": true,
            "message": "chore: 测试 bot",
            "timestamp": "2024-12-10T14:15:11+08:00",
            "url": "https://github.com/Hamster5295/digital-ic-proj/commit/671d0e94215730b2d294e280ce2f03996b213323",
            "author": {
              "name": "Hamster5295",
              "email": "hamster5295@163.com",
              "username": "Hamster5295"
            },
            "committer": {
              "name": "Hamster5295",
              "email": "hamster5295@163.com",
              "username": "Hamster5295"
            },
            "added": [

            ],
            "removed": [

            ],
            "modified": [
              "FSM.v"
            ]
          }
        });

        assert_eq!(
            extract_arg(&val, &"head_commit.message".to_string(), ",").unwrap(),
            "chore: 测试 bot"
        );

        assert_eq!(
            extract_arg(&val, &"head_commit.committer.name".to_string(), ",").unwrap(),
            "Hamster5295"
        );
    }
}

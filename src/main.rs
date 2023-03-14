use serde::{Deserialize, Serialize};
use serde_json;
use std::fs;

#[derive(Serialize, Deserialize)]
struct Config {
    pub qbittorrent_path: String,
    pub save_path: String,
    pub anilist_user_id: i32,
}

#[derive(Serialize, Deserialize)]
struct Variables {
    userID: i32,
}

#[derive(Serialize, Deserialize)]
struct BodyRequest {
    operationName: String,
    variables: Variables,
    query: String,
}

pub fn path_exists(path: &str) -> bool {
    fs::metadata(path).is_ok()
}

const CONFIG_PATH: &str = "./config.json";

#[derive(Debug)]
struct Title {
    romaji: String,
}

#[derive(Debug)]
struct AiringSchedule {
    nodes: Vec<NextAiringEpisode>,
}

#[derive(Debug)]
struct NextAiringEpisode {
    episode: i32,
    timeUntilAiring: i64,
}

#[derive(Debug)]
struct Media {
    id: i32,
    title: Title,
    status: String,
    airingSchedule: AiringSchedule,
    nextAiringEpisode: Option<NextAiringEpisode>,
}

#[derive(Debug)]
struct Anime {
    progress: i32,
    media: Media,
}

#[tokio::main]
async fn main() {
    if !path_exists(CONFIG_PATH) {
        println!("Config file not found, creating one...");
        let config = Config {
            qbittorrent_path: "C:\\Program Files\\qBittorrent\\qbittorrent.exe".to_string(),
            save_path: "D:\\animes".to_string(),
            anilist_user_id: 6204649,
        };

        let json = serde_json::to_string(&config).unwrap();

        fs::write(CONFIG_PATH, json).expect("Unable to write file");
    }

    let contents = fs::read_to_string(CONFIG_PATH).expect("Should have been able to read the file");

    let _json: serde_json::Value =
        serde_json::from_str(&contents).expect("Should have been able to parse the JSON");

    let animes = get_watching_animes(_json["anilist_user_id"].as_i64().unwrap() as i32).await;

    let mut to_watch: Vec<Anime> = vec![];

    for anime in animes {
        if anime.media.status == "RELEASING" {
            if anime.media.nextAiringEpisode.is_some() {
                let next = anime.media.nextAiringEpisode.as_ref();

                match next {
                    Some(next) => {
                        if anime.progress + 1 < next.episode {
                            to_watch.push(anime);
                        }
                    }
                    None => {}
                }
            } else {
                to_watch.push(anime);
            }
        }
    }

    to_watch.iter().for_each(|anime| {
        println!("{} - {}", anime.media.title.romaji, anime.progress);
    });
}

fn scrap_animes(animes: Vec<Anime>) {}

async fn get_watching_animes(anilist_user_id: i32) -> Vec<Anime> {
    let client = reqwest::Client::new();

    let body = BodyRequest {
        operationName: "MyList".to_string(),
        variables: Variables {
            userID: anilist_user_id,
        },
        query: QUERY.to_string(),
    };

    let resp = client
        .post("https://graphql.anilist.co/")
        .json(&body)
        .send()
        .await;

    let body = resp.unwrap().text().await.unwrap();

    let converted_response: serde_json::Value =
        serde_json::from_str(&body).expect("Should have been able to parse the JSON");

    let entries = converted_response["data"]["MediaListCollection"]["lists"][0]["entries"].clone();

    let mut anime_list: Vec<Anime> = vec![];

    for entry in entries.as_array().unwrap() {
        anime_list.push(Anime {
            progress: entry["progress"].as_i64().unwrap() as i32,
            media: Media {
                id: entry["media"]["id"].as_i64().unwrap() as i32,
                title: Title {
                    romaji: entry["media"]["title"]["romaji"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                },

                airingSchedule: AiringSchedule {
                    nodes: entry["media"]["airingSchedule"]["nodes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|node| NextAiringEpisode {
                            episode: node["episode"].as_i64().unwrap() as i32,
                            timeUntilAiring: node["timeUntilAiring"].as_i64().unwrap(),
                        })
                        .collect(),
                },
                status: entry["media"]["status"].as_str().unwrap().to_string(),

                nextAiringEpisode: match entry["media"]["nextAiringEpisode"].is_null() {
                    true => None,
                    false => Some(NextAiringEpisode {
                        episode: entry["media"]["nextAiringEpisode"]["episode"]
                            .as_i64()
                            .unwrap() as i32,
                        timeUntilAiring: entry["media"]["nextAiringEpisode"]["timeUntilAiring"]
                            .as_i64()
                            .unwrap(),
                    }),
                },
            },
        })
    }

    return anime_list;
}

const QUERY: &str = "query MyList {
    MediaListCollection(userId: 6204649, type: ANIME, status: CURRENT) {
      lists {
        name
        entries {
          progress
          media {
            id
            status
            episodes
            airingSchedule {
                nodes{
                  episode
                  timeUntilAiring
                }
              }
            nextAiringEpisode {
              episode
              timeUntilAiring
            }
            title {
              romaji
            }
          }
        }
      }
    }
  }";

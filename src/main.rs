use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json;
use std::fs;
use std::process::Command;

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

struct Torrent {
    magnetic: String,
    name: String,
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

    let torrents = scrap_animes(to_watch).await;

    for torrent in torrents {
        println!("Downloading {}", torrent.name);
        start_qbittorrent(
            &_json["qbittorrent_path"].as_str().unwrap(),
            &_json["save_path"].as_str().unwrap(),
            &torrent.magnetic,
        );
    }

    println!("Have fun!");

    std::process::exit(0);
}

fn start_qbittorrent(path: &str, save_path: &str, magnetic: &str) {
    // TODO save path
    let mut child = Command::new(path)
        .arg("--skip-dialog")
        .arg(&magnetic)
        .spawn();

    match child {
        Ok(mut child) => {
            child.wait().unwrap();
        }
        Err(e) => println!("failed to execute process: {}", e),
    }
}

async fn scrap_animes(animes: Vec<Anime>) -> Vec<Torrent> {
    let mut magnetics: Vec<Torrent> = vec![];

    for anime in animes {
        let next_episode = anime.progress + 1;
        let anime_name = anime.media.title.romaji + " - " + &next_episode.to_string();

        let encoded_name = urlencoding::encode(&anime_name);

        let url = format!(
            "https://nyaa.si/?f=0&c=1_2&q={}&s=seeders&o=desc",
            encoded_name
        );

        let client = reqwest::Client::new();

        println!("Scraping {}", url);
        let resp = client.get(&url).send().await;

        let body = resp.unwrap().text().await.unwrap();

        let document = Html::parse_document(&body);

        let selector = Selector::parse("tbody > tr:first-child").unwrap();

        for element in document.select(&selector) {
            let selector = Selector::parse("td:nth-child(3) > a:nth-child(2)").unwrap();

            let name_selector = Selector::parse("td:nth-child(2) > a:last-child").unwrap();

            let magnetic = element
                .select(&selector)
                .next()
                .unwrap()
                .value()
                .attr("href")
                .unwrap();

            let name = element.select(&name_selector).next().unwrap().inner_html();

            magnetics.push(Torrent {
                magnetic: magnetic.to_string(),
                name: name.to_string(),
            });
        }
    }

    println!("\nFinished Scrapping \n",);
    return magnetics;
}

async fn get_watching_animes(anilist_user_id: i32) -> Vec<Anime> {
    let client = reqwest::Client::new();

    let body = BodyRequest {
        operationName: "MyList".to_string(),
        variables: Variables {
            userID: anilist_user_id,
        },
        query: QUERY.to_string(),
    };

    println!("Getting Watching List for {} \n", anilist_user_id);
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

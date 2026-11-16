use crate::model::Game;
use reqwest::{blocking::Client, redirect::Policy, Url};
use serde::Deserialize;
use std::time::Duration;

const VALORANT: &str = "https://cmsassets.rgpub.io/sanity/images/dsfx7636/news/cbf4460132cdfeb2a97fad5f9dd25ba0bc058f76-128x128.png";
const LEAGUE: &str = "https://www.riotgames.com/darkroom/800/9c08e3b3ce6281f252a4dfbf61357a11:75ab7e60b85225c6c7f19ae4024e27a6/lol-logo-rendered-hi-res.png";

pub fn client() -> Option<Client> {
    Client::builder()
        .https_only(true)
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(7))
        .redirect(Policy::custom(|attempt| {
            let from = attempt.previous().first();
            if attempt.previous().len() < 3
                && from.is_some_and(|from| {
                    steam_art_id(from).is_some_and(|id| steam_art_id(attempt.url()) == Some(id))
                })
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .user_agent(concat!("Afterwork/", env!("CARGO_PKG_VERSION")))
        .build()
        .ok()
}

fn steam_art_id(url: &Url) -> Option<u32> {
    let allowed = base_https(url)
        && matches!(
            url.host_str(),
            Some(
                "shared.steamstatic.com"
                    | "shared.akamai.steamstatic.com"
                    | "shared.fastly.steamstatic.com"
                    | "shared.cloudflare.steamstatic.com"
                    | "cdn.akamai.steamstatic.com"
                    | "cdn.cloudflare.steamstatic.com"
                    | "cdn.fastly.steamstatic.com"
            )
        );
    if !allowed {
        return None;
    }
    let path = url
        .path()
        .strip_prefix("/store_item_assets/steam/apps/")
        .or_else(|| url.path().strip_prefix("/steam/apps/"))?;
    let (id, file) = path.split_once('/')?;
    if ![".jpg", ".jpeg", ".png", ".ico"]
        .iter()
        .any(|ext| file.ends_with(ext))
    {
        return None;
    }
    id.parse().ok()
}

fn base_https(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn response(client: &Client, url: Url, limit: usize) -> Option<Vec<u8>> {
    let result = client.get(url).send().ok()?;
    if !result.status().is_success()
        || result
            .content_length()
            .is_some_and(|size| size > limit as u64)
    {
        return None;
    }
    super::read_limited(result, limit).ok()
}

fn art(client: &Client, url: Url, game_id: u32) -> Option<Vec<u8>> {
    if steam_art_id(&url) != Some(game_id) {
        return None;
    }
    super::normalize(&response(client, url, super::IMAGE_LIMIT)?).ok()
}

fn direct_art(client: &Client, id: u32) -> Option<Vec<u8>> {
    let url = Url::parse(&format!(
        "https://shared.steamstatic.com/store_item_assets/steam/apps/{id}/library_600x900.jpg"
    ))
    .ok()?;
    art(client, url, id).or_else(|| {
        let mut url = Url::parse("https://store.steampowered.com/api/appdetails").ok()?;
        url.query_pairs_mut()
            .append_pair("appids", &id.to_string())
            .append_pair("filters", "basic");
        let data: serde_json::Value =
            serde_json::from_slice(&response(client, url, 768 * 1024)?).ok()?;
        let header = data[id.to_string()]["data"]["header_image"].as_str()?;
        art(client, Url::parse(header).ok()?, id)
    })
}

#[derive(Deserialize)]
struct Search {
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    id: u32,
    name: String,
    tiny_image: String,
}

fn title(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn search(client: &Client, name: &str) -> Option<Vec<u8>> {
    if !(2..=100).contains(&name.chars().count())
        || name.contains(['/', '\\', ':', '@'])
        || name.chars().any(char::is_control)
    {
        return None;
    }
    let title_key = title(name);
    if title_key.len() < 2 {
        return None;
    }
    let mut url = Url::parse("https://store.steampowered.com/api/storesearch/").ok()?;
    url.query_pairs_mut()
        .append_pair("term", name)
        .append_pair("l", "english")
        .append_pair("cc", "US");
    let data: Search = serde_json::from_slice(&response(client, url, 384 * 1024)?).ok()?;
    let mut candidates = data
        .items
        .iter()
        .filter(|item| title(&item.name) == title_key);
    let unique = candidates.next()?;
    if candidates.any(|item| item.id != unique.id) {
        return None;
    }
    art(client, Url::parse(&unique.tiny_image).ok()?, unique.id)
}

pub fn fetch(client: &Client, game: &Game) -> Option<Vec<u8>> {
    let (store, id) = game.catalog_id.split_once(':')?;
    let riot = match (store, id) {
        ("steam", id) => return direct_art(client, id.parse().ok()?),
        ("epic" | "gog" | "ubisoft" | "local", _) => return search(client, &game.name),
        ("riot", "league_of_legends.live") => LEAGUE,
        ("riot", "valorant.live") => VALORANT,
        _ => return None,
    };
    super::normalize(&response(
        client,
        Url::parse(riot).ok()?,
        super::IMAGE_LIMIT,
    )?)
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_recognized_steam_hosts_can_provide_art() {
        for url in [
            "https://shared.steamstatic.com.evil.example/steam/apps/730/header.jpg",
            "https://shared.steamstatic.com@evil.example/steam/apps/730/header.jpg",
            "https://shared.steamstatic.com:8080/steam/apps/730/header.jpg",
            "https://shared.steamstatic.com/steam/apps/730/header.svg",
            "http://shared.steamstatic.com/steam/apps/730/header.jpg",
        ] {
            assert_eq!(steam_art_id(&Url::parse(url).unwrap()), None, "{url}");
        }
        assert_eq!(steam_art_id(&Url::parse("https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/730/hash/header.jpg").unwrap()), Some(730));
    }
}

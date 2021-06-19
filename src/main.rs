 mod app;
 mod cli_app;
 mod config;
 mod network;
 mod redirect_uri;
 mod user_config;

use cli_app::CliApp;
use config::ClientConfig;
use redirect_uri::redirect_uri_web_server;
use rspotify::{
  oauth2::SpotifyOAuth,
  util::{process_token, request_token},
};
use std::error::Error;
use std::io;
use user_config::UserConfig;

use anyhow::Result;
use app::App;

use network::{get_spotify, IoEvent, Network};

use rspotify::oauth2::TokenInfo;
use std::{
  cmp::{max, min},
  io::stdout,
  panic::{self, PanicInfo},
  path::PathBuf,
  sync::Arc,
  time::SystemTime,
};
use tokio::sync::Mutex;

#[derive(Debug)]
pub enum Type {
  Playlist,
  Track,
  Artist,
  Album,
  Show,
  Device,
  Liked,
}

const SCOPES: [&str; 14] = [
  "playlist-read-collaborative",
  "playlist-read-private",
  "playlist-modify-private",
  "playlist-modify-public",
  "user-follow-read",
  "user-follow-modify",
  "user-library-modify",
  "user-library-read",
  "user-modify-playback-state",
  "user-read-currently-playing",
  "user-read-playback-state",
  "user-read-playback-position",
  "user-read-private",
  "user-read-recently-played",
];

/// get token automatically with local webserver
pub async fn get_token_auto(spotify_oauth: &mut SpotifyOAuth, port: u16) -> Option<TokenInfo> {
  match spotify_oauth.get_cached_token().await {
    Some(token_info) => Some(token_info),
    None => match redirect_uri_web_server(spotify_oauth, port) {
      Ok(mut url) => process_token(spotify_oauth, &mut url).await,
      Err(()) => {
        println!("Starting webserver failed. Continuing with manual authentication");
        request_token(spotify_oauth);
        println!("Enter the URL you were redirected to: ");
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
          Ok(_) => process_token(spotify_oauth, &mut input).await,
          Err(_) => None,
        }
      }
    },
  }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let song_name = "Anchor - Josh Garells";
  let mut user_config = UserConfig::new();
  user_config.load_config()?;

  let mut client_config = ClientConfig::new();
  client_config.load_config()?;

  let config_paths = client_config.get_or_build_paths()?;

  // Start authorization with spotify
  let mut oauth = SpotifyOAuth::default()
    .client_id(&client_config.client_id)
    .client_secret(&client_config.client_secret)
    .redirect_uri(&client_config.get_redirect_uri())
    .cache_path(config_paths.token_cache_path)
    .scope(&SCOPES.join(" "))
    .build();

  let config_port = client_config.get_port();

  match get_token_auto(&mut oauth, config_port).await {
    Some(token_info) => {
      let (sync_io_tx, sync_io_rx) = std::sync::mpsc::channel::<IoEvent>();

      let (spotify, token_expiry) = get_spotify(token_info);

      // Initialise app state
      let app = Arc::new(Mutex::new(App::new(
        sync_io_tx,
        user_config.clone(),
        token_expiry,
      )));

      let network = Network::new(oauth, spotify, client_config, &app);
      let mut cli = CliApp::new(network, user_config);

      cli.net.handle_network_event(IoEvent::GetDevices).await;
      cli
        .net
        .handle_network_event(IoEvent::GetCurrentPlayback)
        .await;

      let devices_list = match &cli.net.app.lock().await.devices {
        Some(p) => p
          .devices
          .iter()
          .map(|d| d.id.clone())
          .collect::<Vec<String>>(),
        None => Vec::new(),
      };

      // If the device_id is not specified, select the first available device
      let device_id = cli.net.client_config.device_id.clone();
      if device_id.is_none() || !devices_list.contains(&device_id.unwrap()) {
        // Select the first device available
        if let Some(d) = devices_list.get(0) {
          cli.net.client_config.set_device_id(d.clone())?;
        }
      }
      //arg3 will add to queue.!!!!!
      cli
        .play(song_name.to_string(), Type::Track, false, false)
        .await?;
    }
    None => println!("\nSpotify auth failed"),
  }

  Ok(())
}

use crate::network::{IoEvent, Network};
use crate::user_config::UserConfig;

use super::Type;

use anyhow::{anyhow, Result};
use rand::{thread_rng, Rng};

pub struct CliApp<'a> {
  pub net: Network<'a>,
  pub config: UserConfig,
}

// Non-concurrent functions
// I feel that async in a cli is not working
// I just .await all processes and directly interact
// by calling network.handle_network_event
impl<'a> CliApp<'a> {
  pub fn new(net: Network<'a>, config: UserConfig) -> Self {
    Self { net, config }
  }

  // spt play -u URI
  pub async fn play_uri(&mut self, uri: String, queue: bool, random: bool) {
    let offset = if random {
      // Only works with playlists for now
      if uri.contains("spotify:playlist:") {
        let id = uri.split(':').last().unwrap();
        match self.net.spotify.playlist(id, None, None).await {
          Ok(p) => {
            let num = p.tracks.total;
            Some(thread_rng().gen_range(0..num) as usize)
          }
          Err(e) => {
            self
              .net
              .app
              .lock()
              .await
              .handle_error(anyhow!(e.to_string()));
            return;
          }
        }
      } else {
        None
      }
    } else {
      None
    };

    if uri.contains("spotify:track:") {
      if queue {
        self
          .net
          .handle_network_event(IoEvent::AddItemToQueue(uri))
          .await;
      } else {
        self
          .net
          .handle_network_event(IoEvent::StartPlayback(
            None,
            Some(vec![uri.clone()]),
            Some(0),
          ))
          .await;
      }
    } else {
      self
        .net
        .handle_network_event(IoEvent::StartPlayback(Some(uri.clone()), None, offset))
        .await;
    }
  }

  // spt play -n NAME ...
  pub async fn play(&mut self, name: String, item: Type, queue: bool, random: bool) -> Result<()> {
    self
      .net
      .handle_network_event(IoEvent::GetSearchResults(name.clone(), None))
      .await;
    // Get the uri of the first found
    // item + the offset or return an error message
    let uri = {
      let results = &self.net.app.lock().await.search_results;
      match item {
        Type::Track => {
          if let Some(r) = &results.tracks {
            r.items[0].uri.clone()
          } else {
            return Err(anyhow!("no tracks with name '{}'", name));
          }
        }
        Type::Album => {
          if let Some(r) = &results.albums {
            let album = &r.items[0];
            if let Some(uri) = &album.uri {
              uri.clone()
            } else {
              return Err(anyhow!("album {} has no uri", album.name));
            }
          } else {
            return Err(anyhow!("no albums with name '{}'", name));
          }
        }
        Type::Artist => {
          if let Some(r) = &results.artists {
            r.items[0].uri.clone()
          } else {
            return Err(anyhow!("no artists with name '{}'", name));
          }
        }
        Type::Show => {
          if let Some(r) = &results.shows {
            r.items[0].uri.clone()
          } else {
            return Err(anyhow!("no shows with name '{}'", name));
          }
        }
        Type::Playlist => {
          if let Some(r) = &results.playlists {
            let p = &r.items[0];
            // For a random song, create a random offset
            p.uri.clone()
          } else {
            return Err(anyhow!("no playlists with name '{}'", name));
          }
        }
        _ => unreachable!(),
      }
    };

    // Play or queue the uri
    self.play_uri(uri, queue, random).await;

    Ok(())
  }
}

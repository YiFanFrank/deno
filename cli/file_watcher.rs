use crate::colors;
use deno_core::ErrBox;
use futures::stream::StreamExt;
use futures::Future;
use notify::event::Event as NotifyEvent;
use notify::event::EventKind;
use notify::Config;
use notify::Error as NotifyError;
use notify::RecommendedWatcher;
use notify::RecursiveMode;
use notify::Watcher;
use std::path::PathBuf;
use std::pin::Pin;
use tokio::select;
use tokio::sync::mpsc;

// TODO(bartlomieju): rename
type WatchFuture =
  Pin<Box<dyn Future<Output = std::result::Result<(), deno_core::ErrBox>>>>;

async fn error_handler(watch_future: WatchFuture) {
  let result = watch_future.await;
  if let Err(err) = result {
    let msg = format!("{}: {}", colors::red_bold("error"), err.to_string(),);
    eprintln!("{}", msg);
  }
}

pub async fn watch_func<F>(
  watch_paths: &[PathBuf],
  closure: F,
) -> Result<(), ErrBox>
where
  F: Fn() -> WatchFuture,
{
  loop {
    let func = error_handler(closure());
    let mut is_file_changed = false;
    select! {
      _ = file_watcher(watch_paths) => {
          is_file_changed = true;
          info!(
            "{} File change detected! Restarting!",
            colors::intense_blue("Watcher")
          );
        },
      _ = func => { },
    };
    if !is_file_changed {
      info!(
        "{} Process terminated! Restarting on file change...",
        colors::intense_blue("Watcher")
      );
      file_watcher(watch_paths).await?;
      info!(
        "{} File change detected! Restarting!",
        colors::intense_blue("Watcher")
      );
    }
  }
}

pub async fn file_watcher(paths: &[PathBuf]) -> Result<(), deno_core::ErrBox> {
  let (sender, mut receiver) = mpsc::channel::<Result<NotifyEvent, ErrBox>>(16);
  let sender = std::sync::Mutex::new(sender);

  let mut watcher: RecommendedWatcher =
    Watcher::new_immediate(move |res: Result<NotifyEvent, NotifyError>| {
      let res2 = res.map_err(ErrBox::from);
      let mut sender = sender.lock().unwrap();
      // Ignore result, if send failed it means that watcher was already closed,
      // but not all messages have been flushed.
      let _ = sender.try_send(res2);
    })?;

  watcher.configure(Config::PreciseEvents(true)).unwrap();

  for path in paths {
    watcher.watch(path, RecursiveMode::NonRecursive)?;
  }

  while let Some(result) = receiver.next().await {
    let event = result?;
    match event.kind {
      EventKind::Create(_) => break,
      EventKind::Modify(_) => break,
      EventKind::Remove(_) => break,
      _ => continue,
    }
  }
  Ok(())
}

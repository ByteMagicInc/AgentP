use anyhow::{Result, bail};
use tokio::sync::mpsc;

use crate::podcast::{
    Config, DownloadEvent, Podcast, config_dir, config_path, download_selected_episodes,
    fetch_episode_list, fetch_feed_metadata, get_last_podcast_name, load_config, sanitize_filename,
    save_config,
};

use super::args::*;

fn resolve_podcast(config: &Config, selector: &str) -> Result<usize> {
    let lower = selector.to_lowercase();
    if let Some(i) = config
        .podcasts
        .iter()
        .position(|p| p.name.to_lowercase() == lower)
    {
        return Ok(i);
    }
    if let Ok(n) = selector.parse::<usize>() {
        if n == 0 || n > config.podcasts.len() {
            bail!(
                "Podcast index {} is out of range (1–{})",
                n,
                config.podcasts.len()
            );
        }
        return Ok(n - 1);
    }
    bail!("No podcast found with name \"{}\"", selector)
}

fn bool_label(v: bool) -> &'static str {
    if v { "on" } else { "off" }
}

pub async fn run(command: Command) -> Result<()> {
    match command {
        Command::Podcast { command } => run_podcast(command).await,
        Command::Download(args) => run_download(args).await,
        Command::Config { command } => run_config(command),
        Command::Open { command } => run_open(command),
    }
}

async fn run_podcast(command: PodcastCommand) -> Result<()> {
    match command {
        PodcastCommand::List(args) => podcast_list(args).await,
        PodcastCommand::Show(args) => podcast_show(args).await,
        PodcastCommand::Episodes(args) => podcast_episodes(args).await,
        PodcastCommand::Add(args) => podcast_add(args).await,
        PodcastCommand::Edit(args) => podcast_edit(args),
        PodcastCommand::Delete(args) => podcast_delete(args),
    }
}

async fn podcast_list(args: ListPodcastArgs) -> Result<()> {
    let (config, _) = load_config()?;
    if config.podcasts.is_empty() {
        println!("No podcasts configured.");
        return Ok(());
    }
    if args.no_fetch {
        for (i, podcast) in config.podcasts.iter().enumerate() {
            println!("{}. {}", i + 1, podcast.name);
        }
        return Ok(());
    }
    let mut set = tokio::task::JoinSet::new();
    for (i, podcast) in config.podcasts.iter().enumerate() {
        let p = podcast.clone();
        set.spawn(async move {
            let latest = match get_last_podcast_name(&p).await {
                Ok((title, date)) => {
                    let date_str = date.unwrap_or_default();
                    if date_str.is_empty() {
                        title
                    } else {
                        format!("{} ({})", title, date_str)
                    }
                }
                Err(_) => "(could not fetch latest)".to_string(),
            };
            (i, p.name, latest)
        });
    }
    let mut results = set.join_all().await;
    results.sort_by_key(|(i, _, _)| *i);
    for (i, name, latest) in results {
        println!("{}. {} — {}", i + 1, name, latest);
    }
    Ok(())
}

async fn podcast_show(args: ShowPodcastArgs) -> Result<()> {
    let (config, _) = load_config()?;
    let idx = resolve_podcast(&config, &args.podcast)?;
    let p = &config.podcasts[idx];

    println!("Name:                  {}", p.name);
    println!("Feed URL:              {}", p.feed_url);
    println!("Album name:            {}", p.effective_album_name());
    if !p.artist.is_empty() {
        println!("Artist:                {}", p.artist);
    }
    println!("Write tags:            {}", bool_label(p.overwrite_tags));
    println!(
        "Remove existing tags:  {}",
        bool_label(p.remove_existing_tags)
    );
    println!("Remove images:         {}", bool_label(p.remove_images));
    println!(
        "Write album tag:       {}",
        bool_label(p.overwrite_album_name)
    );
    println!("Write artist tag:      {}", bool_label(p.overwrite_artist));
    println!("Write title tag:       {}", bool_label(p.overwrite_title));
    println!(
        "Rename files:          {}",
        bool_label(p.overwrite_file_name)
    );
    println!(
        "Append number:         {}",
        bool_label(p.append_number_to_title)
    );
    println!("Leading zeros:         {}", p.leading_zeros_amount);

    let folder = std::path::Path::new(&config.download_dir_location).join(p.effective_album_name());
    println!("Download folder:       {}", folder.display());

    match get_last_podcast_name(p).await {
        Ok((title, date)) => {
            let date_str = date.unwrap_or_default();
            if date_str.is_empty() {
                println!("Latest episode:        {}", title);
            } else {
                println!("Latest episode:        {} ({})", title, date_str);
            }
        }
        Err(_) => println!("Latest episode:        (could not fetch)"),
    }

    Ok(())
}

async fn podcast_episodes(args: EpisodesArgs) -> Result<()> {
    let (config, _) = load_config()?;
    let idx = resolve_podcast(&config, &args.podcast)?;
    let podcast = &config.podcasts[idx];
    let episodes = fetch_episode_list(&podcast.feed_url).await?;
    if episodes.is_empty() {
        println!("No episodes found for \"{}\".", podcast.name);
        return Ok(());
    }
    let show: &[_] = if let Some(n) = args.limit {
        &episodes[..n.min(episodes.len())]
    } else {
        &episodes
    };
    println!(
        "{} episodes for \"{}\"{}:",
        episodes.len(),
        podcast.name,
        if args.limit.is_some() {
            format!(" (showing {})", show.len())
        } else {
            String::new()
        }
    );
    for ep in show {
        let date = ep.pub_date.as_deref().unwrap_or("");
        println!("{:>4}  {}  {}", ep.feed_index, date, ep.title);
    }
    Ok(())
}

fn apply_podcast_fields(podcast: &mut Podcast, fields: PodcastFieldArgs) {
    if let Some(v) = fields.album_name {
        podcast.album_name = v;
    }
    if let Some(v) = fields.artist {
        podcast.artist = v;
    }
    if let Some(v) = fields.user_agent {
        podcast.user_agent = v;
    }
    if let Some(v) = fields.overwrite_tags {
        podcast.overwrite_tags = v;
    }
    if let Some(v) = fields.remove_existing_tags {
        podcast.remove_existing_tags = v;
    }
    if let Some(v) = fields.remove_images {
        podcast.remove_images = v;
    }
    if let Some(v) = fields.overwrite_album_name {
        podcast.overwrite_album_name = v;
    }
    if let Some(v) = fields.overwrite_artist {
        podcast.overwrite_artist = v;
    }
    if let Some(v) = fields.overwrite_file_name {
        podcast.overwrite_file_name = v;
    }
    if let Some(v) = fields.overwrite_title {
        podcast.overwrite_title = v;
    }
    if let Some(v) = fields.append_number_to_title {
        podcast.append_number_to_title = v;
    }
    if let Some(v) = fields.leading_zeros_amount {
        podcast.leading_zeros_amount = v;
    }
}

async fn podcast_add(args: AddPodcastArgs) -> Result<()> {
    let (mut config, _) = load_config()?;
    let mut podcast = config.default_podcast.clone();
    podcast.feed_url = args.feed_url.clone();

    if args.from_feed {
        let meta = fetch_feed_metadata(&args.feed_url).await?;
        podcast.name = args.name.unwrap_or_else(|| meta.title.clone());
        if podcast.album_name.is_empty() {
            podcast.album_name = sanitize_filename(&meta.title);
        }
        if podcast.artist.is_empty() && !meta.author.is_empty() {
            podcast.artist = meta.author;
        }
    } else {
        let Some(name) = args.name else {
            bail!("--name is required unless --from-feed is specified");
        };
        podcast.name = name;
    }

    apply_podcast_fields(&mut podcast, args.fields);
    let name = podcast.name.clone();
    config.podcasts.push(podcast);
    save_config(&config)?;
    println!("Added podcast \"{}\".", name);
    Ok(())
}

fn podcast_edit(args: EditPodcastArgs) -> Result<()> {
    let (mut config, _) = load_config()?;
    let idx = resolve_podcast(&config, &args.podcast)?;
    let podcast = &mut config.podcasts[idx];
    if let Some(v) = args.name {
        podcast.name = v;
    }
    if let Some(v) = args.feed_url {
        podcast.feed_url = v;
    }
    apply_podcast_fields(podcast, args.fields);
    save_config(&config)?;
    println!("Updated podcast \"{}\".", config.podcasts[idx].name);
    Ok(())
}

fn podcast_delete(args: DeletePodcastArgs) -> Result<()> {
    let (mut config, _) = load_config()?;
    let idx = resolve_podcast(&config, &args.podcast)?;
    let name = config.podcasts[idx].name.clone();
    config.podcasts.remove(idx);
    save_config(&config)?;
    println!("Deleted podcast \"{}\".", name);
    Ok(())
}

async fn run_download(args: DownloadArgs) -> Result<()> {
    let (config, _) = load_config()?;
    let idx = resolve_podcast(&config, &args.podcast)?;
    let podcast = &config.podcasts[idx];
    let dir = config.download_dir_location.clone();

    let episodes = if let Some(indices) = args.episodes {
        indices
    } else if args.latest.is_some() || args.oldest.is_some() || args.all {
        let feed_episodes = fetch_episode_list(&podcast.feed_url).await?;
        if feed_episodes.is_empty() {
            bail!("No episodes found for \"{}\"", podcast.name);
        }
        let total = feed_episodes.len();
        if args.all {
            feed_episodes.iter().map(|ep| ep.feed_index).collect()
        } else if let Some(n) = args.latest {
            feed_episodes
                .iter()
                .take(n.min(total))
                .map(|ep| ep.feed_index)
                .collect()
        } else if let Some(n) = args.oldest {
            feed_episodes
                .iter()
                .rev()
                .take(n.min(total))
                .map(|ep| ep.feed_index)
                .collect()
        } else {
            vec![]
        }
    } else {
        bail!("Specify episodes with --episodes, --latest, --oldest, or --all");
    };

    if episodes.is_empty() {
        println!("No episodes to download.");
        return Ok(());
    }

    println!(
        "Downloading {} episode(s) from \"{}\"...",
        episodes.len(),
        podcast.name
    );

    let (tx, mut rx) = mpsc::unbounded_channel();
    let podcast_clone = podcast.clone();

    tokio::spawn(async move {
        if let Err(e) = download_selected_episodes(&dir, &podcast_clone, episodes, tx.clone()).await
        {
            let _ = tx.send(DownloadEvent::Error {
                message: e.to_string(),
            });
            let _ = tx.send(DownloadEvent::Finished);
        }
    });

    let mut had_error = false;
    while let Some(event) = rx.recv().await {
        match event {
            DownloadEvent::Started {
                episode_name,
                index,
                total,
            } => {
                println!("[{}/{}] Downloading: {}", index, total, episode_name);
            }
            DownloadEvent::Completed { episode_name } => {
                println!("  Downloaded: {}", episode_name);
            }
            DownloadEvent::Tagged { episode_name } => {
                println!("  Tagged: {}", episode_name);
            }
            DownloadEvent::Error { message } => {
                eprintln!("  Error: {}", message);
                had_error = true;
            }
            DownloadEvent::Finished => break,
        }
    }

    if had_error {
        bail!("One or more episodes failed to download");
    }
    println!("Done.");
    Ok(())
}

fn run_config(command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Show => {
            let (config, _) = load_config()?;
            println!("download_dir:   {}", config.download_dir_location);
            println!("default_mode:   {}", config.default_mode);
            println!("banner_style:   {}", config.banner_style);
            println!("podcasts:       {}", config.podcasts.len());
            println!();
            println!("Default podcast template:");
            let d = &config.default_podcast;
            println!("  write_tags:            {}", bool_label(d.overwrite_tags));
            println!(
                "  remove_existing_tags:  {}",
                bool_label(d.remove_existing_tags)
            );
            println!("  remove_images:         {}", bool_label(d.remove_images));
            println!(
                "  write_album_tag:       {}",
                bool_label(d.overwrite_album_name)
            );
            println!(
                "  write_artist_tag:      {}",
                bool_label(d.overwrite_artist)
            );
            println!("  write_title_tag:       {}", bool_label(d.overwrite_title));
            println!(
                "  rename_files:          {}",
                bool_label(d.overwrite_file_name)
            );
            println!(
                "  append_number:         {}",
                bool_label(d.append_number_to_title)
            );
            println!("  leading_zeros:         {}", d.leading_zeros_amount);
            println!();
            println!("Config files:");
            println!(
                "  config.json:   {}",
                config_path().map_or("(unknown)".into(), |p| p.display().to_string())
            );
            println!(
                "  podcasts.json: {}",
                config_dir()
                    .map(|d| d.join("podcasts.json"))
                    .map_or("(unknown)".into(), |p| p.display().to_string())
            );
            Ok(())
        }
        ConfigCommand::Set(args) => {
            let (mut config, _) = load_config()?;
            if let Some(dir) = args.download_dir {
                config.download_dir_location = dir;
            }
            if let Some(mode) = args.default_mode {
                config.default_mode = mode;
            }
            if let Some(style) = args.banner_style {
                config.banner_style = style;
            }
            save_config(&config)?;
            println!("Config updated.");
            Ok(())
        }
        ConfigCommand::Path => {
            let dir = config_dir()?;
            println!("{}", dir.display());
            Ok(())
        }
    }
}

fn run_open(command: OpenCommand) -> Result<()> {
    match command {
        OpenCommand::Downloads { create } => {
            let (config, _) = load_config()?;
            let path = std::path::Path::new(&config.download_dir_location);
            if !path.exists() {
                if create {
                    std::fs::create_dir_all(path)?;
                } else {
                    bail!(
                        "Download folder does not exist: {} — use --create to create it",
                        config.download_dir_location
                    );
                }
            }
            open::that(&config.download_dir_location)?;
            Ok(())
        }
        OpenCommand::Podcast(args) => {
            let (config, _) = load_config()?;
            let idx = resolve_podcast(&config, &args.podcast)?;
            let podcast = &config.podcasts[idx];
            let album = podcast.effective_album_name();
            let folder = std::path::Path::new(&config.download_dir_location).join(album);
            if !folder.exists() {
                if args.create {
                    std::fs::create_dir_all(&folder)?;
                } else {
                    bail!(
                        "Podcast folder does not exist: {} — use --create to create it",
                        folder.display()
                    );
                }
            }
            open::that(&folder)?;
            Ok(())
        }
    }
}

use std::{collections::HashMap, error::Error, ops::Deref};

use clap::{builder::PossibleValue, Args, Parser, Subcommand, ValueEnum};
use sunflower_daemon_proto::{
    PlayerRequest, ProviderConfig, RepeatState as ProtoRepeatState, RequestPayload, RequestType,
    TrackConfig, TrackData, TrackSearch,
};

#[derive(Debug, Clone)]
struct RepeatState(ProtoRepeatState);

impl Deref for RepeatState {
    type Target = ProtoRepeatState;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ValueEnum for RepeatState {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self(ProtoRepeatState::None),
            Self(ProtoRepeatState::Track),
            Self(ProtoRepeatState::Queue),
        ]
    }

    fn from_str(input: &str, ignore_case: bool) -> Result<Self, String> {
        let mut input = input.to_string();
        if ignore_case {
            input = input.to_lowercase();
        }

        match input.as_str() {
            "none" => Ok(Self(ProtoRepeatState::None)),
            "track" => Ok(Self(ProtoRepeatState::Track)),
            "queue" => Ok(Self(ProtoRepeatState::Queue)),
            _ => Err(format!("Invalid repeat state: {}", input)),
        }
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        let value = match self.0 {
            ProtoRepeatState::None => "none",
            ProtoRepeatState::Track => "track",
            ProtoRepeatState::Queue => "queue",
        };

        Some(PossibleValue::new(value))
    }
}

/// Parse a single key-value pair
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

#[derive(Args, Debug)]
#[group(required = true, multiple = false)]
struct TrackAddOption {
    /// Track id. Can be found in search result
    #[arg(long)]
    id: Option<String>,

    /// Add track with track config, should be a key-value pair, e.g.:
    ///
    /// --config foo=bar --config baz=qux
    #[arg(long, short, value_parser=parse_key_val::<String, String>,  )]
    config: Option<Vec<(String, String)>>,
}

#[derive(Args, Debug)]
#[group(required = true, multiple = false)]
struct TrackRemoveOption {
    /// Remove all tracks
    #[arg(short, long)]
    all: bool,

    /// Specify track index to remove
    index: Option<u16>,
}

#[derive(Debug, Subcommand)]
enum TrackSubcommands {
    /// Add track to queue with given provider and track id
    Add {
        /// Provider name
        provider: String,

        #[command(flatten)]
        opt: TrackAddOption,
    },
    /// Remove track from queue with given index
    Remove {
        #[command(flatten)]
        opt: TrackRemoveOption,
    },
}

#[derive(Debug, Subcommand)]
enum ProviderSubcommands {
    /// Register a new provider
    New {
        /// Provider name
        name: String,
    },
    /// Unregister a provider
    Remove,
    /// Print all available providers
    Available,
    /// Print all registered providers
    Registered,
    /// Search keyword with given providers
    Search {
        /// Search keyword with all providers
        #[arg(short, long)]
        all: bool,

        /// Keyword
        keyword: String,

        /// Search result amount
        amount: Option<u32>,
    },
}

#[derive(Debug, Subcommand)]
enum Subcommands {
    /// Check if the daemon is alive.
    Check,
    /// Play track
    Play,
    /// Pause track
    Pause,
    /// Stop track
    Stop,
    /// Next track
    Next,
    /// Previous track
    Prev,
    /// Print current repeat mode or set with given mode
    Repeat {
        /// Repeat mode
        #[arg(value_enum)]
        state: Option<RepeatState>,
    },
    /// Print current volume or set with given value
    Volume {
        #[arg(value_parser= clap::value_parser!(u16).range(0..=100))]
        volume: Option<u16>,
    },
    /// Toggle shuffle mode
    ToggleShuffle,
    /// Print current daemon status
    Status,

    /// Track subcommands
    Track {
        #[command(subcommand)]
        cmd: TrackSubcommands,
    },

    /// Provider subcommands
    Provider {
        #[command(subcommand)]
        cmd: ProviderSubcommands,
    },
    /// Ciallo～(∠・ω< )⌒★
    Magic,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum SendMethod {
    Tcp,

    #[cfg(unix)]
    UnixSocket,

    #[cfg(windows)]
    WindowsNamedPipe,
}

#[derive(Debug, Parser)]
pub struct CmdOptions {
    #[command(subcommand)]
    subcmd: Subcommands,

    #[arg(long, short, value_enum)]
    pub method: Option<SendMethod>,
}

impl CmdOptions {
    pub fn build_request(self) -> PlayerRequest {
        match self.subcmd {
            Subcommands::Check => PlayerRequest {
                r#type: RequestType::CheckAlive.into(),
                payload: None,
            },
            Subcommands::Play => PlayerRequest {
                r#type: RequestType::Play.into(),
                payload: None,
            },
            Subcommands::Pause => PlayerRequest {
                r#type: RequestType::Pause.into(),
                payload: None,
            },
            Subcommands::Stop => PlayerRequest {
                r#type: RequestType::Stop.into(),
                payload: None,
            },
            Subcommands::Next => PlayerRequest {
                r#type: RequestType::Next.into(),
                payload: None,
            },
            Subcommands::Prev => PlayerRequest {
                r#type: RequestType::Prev.into(),
                payload: None,
            },
            Subcommands::Repeat { state } => {
                if let Some(state) = state {
                    PlayerRequest {
                        r#type: RequestType::SetRepeat.into(),
                        payload: Some(RequestPayload::RepeatState((*state.deref()).into())),
                    }
                } else {
                    PlayerRequest {
                        r#type: RequestType::GetRepeat.into(),
                        payload: None,
                    }
                }
            }
            Subcommands::Volume { volume: value } => {
                if let Some(volume) = value {
                    PlayerRequest {
                        r#type: RequestType::SetVolume.into(),
                        payload: Some(RequestPayload::Data(format!("{}", volume as f32 / 100.0))),
                    }
                } else {
                    PlayerRequest {
                        r#type: RequestType::GetVolume.into(),
                        payload: None,
                    }
                }
            }
            Subcommands::ToggleShuffle => PlayerRequest {
                r#type: RequestType::ToggleShuffle.into(),
                payload: None,
            },
            Subcommands::Status => PlayerRequest {
                r#type: RequestType::GetStatus.into(),
                payload: None,
            },
            Subcommands::Track { cmd } => match cmd {
                TrackSubcommands::Add { provider, opt } => {
                    if let Some(id) = opt.id {
                        PlayerRequest {
                            r#type: RequestType::AddTrack.into(),
                            payload: Some(RequestPayload::Track(TrackData { provider, id })),
                        }
                    } else if let Some(config) = opt.config {
                        PlayerRequest {
                            r#type: RequestType::AddTrackFromConfig.into(),
                            payload: Some(RequestPayload::TrackConfig(TrackConfig {
                                provider,
                                config: HashMap::from_iter(config),
                            })),
                        }
                    } else {
                        unreachable!()
                    }
                }
                TrackSubcommands::Remove { opt } => {
                    if opt.all {
                        PlayerRequest {
                            r#type: RequestType::ClearQueue.into(),
                            payload: None,
                        }
                    } else {
                        PlayerRequest {
                            r#type: RequestType::RemoveTrack.into(),
                            payload: Some(RequestPayload::Data(format!("{}", opt.index.unwrap()))),
                        }
                    }
                }
            },

            Subcommands::Provider { cmd } => match cmd {
                ProviderSubcommands::New { name } => PlayerRequest {
                    r#type: RequestType::NewProvider.into(),
                    payload: Some(RequestPayload::ProviderConfig(ProviderConfig {
                        config: HashMap::from_iter(vec![("provider_name".to_string(), name)]),
                    })),
                },
                ProviderSubcommands::Remove => PlayerRequest {
                    r#type: RequestType::RemoveProvider.into(),
                    payload: None,
                },
                ProviderSubcommands::Available => PlayerRequest {
                    r#type: RequestType::AvailableProviders.into(),
                    payload: None,
                },
                ProviderSubcommands::Registered => PlayerRequest {
                    r#type: RequestType::RegisteredProviders.into(),
                    payload: None,
                },
                ProviderSubcommands::Search {
                    all,
                    keyword,
                    amount,
                } => {
                    let ty = if all {
                        RequestType::ProviderSearchAll
                    } else {
                        RequestType::ProviderSearch
                    };

                    PlayerRequest {
                        r#type: ty.into(),
                        payload: Some(RequestPayload::TrackSearch(TrackSearch {
                            providers: Vec::new(),
                            query: keyword,
                            amount: amount.unwrap_or(1),
                        })),
                    }
                }
            },
            Subcommands::Magic => PlayerRequest {
                r#type: RequestType::SecretCode.into(),
                payload: None,
            },
        }
    }
}

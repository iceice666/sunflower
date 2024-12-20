use crate::player::Repeat;
use crate::provider::local_file::LocalFileProvider;
use crate::provider::ytdl::YtdlProvider;
use crate::provider::*;

pub(crate) mod proto {
    tonic::include_proto!("player.v1");
}

impl From<Repeat> for proto::player_state::RepeatMode {
    fn from(value: Repeat) -> Self {
        match value {
            Repeat::None => Self::RepeatOff,
            Repeat::Track => Self::RepeatOne,
            Repeat::Queue => Self::RepeatAll,
        }
    }
}

impl From<proto::player_state::RepeatMode> for Repeat {
    fn from(value: proto::player_state::RepeatMode) -> Self {
        match value {
            proto::player_state::RepeatMode::RepeatOff => Repeat::None,
            proto::player_state::RepeatMode::RepeatOne => Repeat::Track,
            proto::player_state::RepeatMode::RepeatAll => Repeat::Queue,
        }
    }
}

impl TryFrom<proto::register_provider_request::Provider> for ProviderKinds {
    type Error = String;

    fn try_from(value: proto::register_provider_request::Provider) -> Result<Self, Self::Error> {
        match value {
            proto::register_provider_request::Provider::Sinewave(proto::SineWaveProvider {}) => {
                Ok(ProviderKinds::Sinewave(Default::default()))
            }

            proto::register_provider_request::Provider::LocalFile(proto::LocalFileProvider {
                music_folder,
                recursive_scan,
            }) => Ok(ProviderKinds::LocalFile(LocalFileProvider::new(
                music_folder,
                recursive_scan,
            ))),

            proto::register_provider_request::Provider::Ytdl(proto::YtdlProvider {
                binary_path: Some(binary_path),
                extra_args,
            }) => Ok(ProviderKinds::Ytdl(
                YtdlProvider::try_new(binary_path, extra_args).map_err(|e| e.to_string())?,
            )),

            _ => Err(String::from("Unsupported provider type")),
        }
    }
}

use crate::source::error::SourceResult;
use crate::source::local_file::LocalFileTrack;
use crate::source::sinewave::SineWaveTrack;
use std::collections::HashMap;
use std::fmt::Debug;
use std::time::Duration;

pub mod error;

pub mod local_file;
pub mod sinewave;

type TrackSourceType<T> = Box<dyn rodio::Source<Item = T> + Send + Sync>;

pub enum RawAudioSource {
    F32(TrackSourceType<f32>),
    I16(TrackSourceType<i16>),
    U16(TrackSourceType<u16>),
}

impl RawAudioSource {
    pub fn total_duration(&self) -> Option<Duration> {
        match self {
            Self::F32(track_source) => track_source.total_duration(),
            Self::I16(track_source) => track_source.total_duration(),
            Self::U16(track_source) => track_source.total_duration(),
        }
    }
}

pub type SourceInfo = HashMap<String, String>;

/// Trait representing a generic audio source which can be used
/// to gather metadata and build audio sources.
pub trait SourceTrait: Send + Sync + Debug + PartialEq {
    /// Retrieves information about the audio source.
    ///
    /// # Returns
    /// A `SourceResult` wrapping a `HashMap` where the keys and values
    /// contain metadata about the audio source.
    fn info(&self) -> SourceResult<SourceInfo>;

    /// Builds the raw audio source from the implementing type.
    ///
    /// # Returns
    /// A `SourceResult` wrapping a `RawAudioSource` which can be
    /// used for playback.
    fn build_source(&self) -> SourceResult<RawAudioSource>;

    /// Obtains a unique identifier for the audio source.
    ///
    /// # Returns
    /// A `String` that uniquely identifies the audio source.
    fn get_unique_id(&self) -> String;

    /// Displays a title for the audio source.
    ///
    /// This method returns the unique identifier by default but can
    /// be overridden to provide a more descriptive title.
    ///
    /// # Returns
    /// A `String` representing the title of the audio source.
    fn display_title(&self) -> String {
        self.get_unique_id()
    }
}

macro_rules! define_source_kinds {
    (
        $f_name:ident=>$f_clz:ident
        $(,$name:ident=>$clz:ident)*

    ) => {
        #[derive(Debug,  PartialEq,)]
        pub enum SourceKinds{
            $f_name($f_clz)
            $(,$name ($clz))*
        }

        impl From<$f_clz> for SourceKinds {
            fn from(value: $f_clz) -> Self {
                SourceKinds::$f_name(value)
            }
        }

        $(
        impl From<$clz> for SourceKinds {
            fn from(value: $clz) -> Self {
                SourceKinds::$name(value)
            }
        }
        )*

        impl SourceKinds {
            pub fn get_source_kind(&self) -> &'static str {
                match self {
                    Self::$f_name(_) => stringify!($f_name)
                    $(,Self::$name(_) => stringify!($name))*
                }
            }
        }

        impl SourceTrait for SourceKinds {

            fn info(&self) -> SourceResult<SourceInfo> {
                match self {
                    Self::$f_name(kind) => kind.info()
                    $(,Self::$name(kind) => kind.info())*
                }
            }

            fn build_source(&self) -> SourceResult<RawAudioSource> {
                match self {
                    Self::$f_name(kind) => kind.build_source()
                    $(,Self::$name(kind) => kind.build_source())*
                }
            }

            fn get_unique_id(&self) -> String {
                match self {
                    Self::$f_name(kind) => kind.get_unique_id()
                    $(,Self::$name(kind) => kind.get_unique_id())*
                }
            }
        }
    };
}

define_source_kinds! {
    Sinwave=>SineWaveTrack,
    Local=>LocalFileTrack
}

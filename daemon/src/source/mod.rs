use crate::source::error::SourceResult;
use crate::source::local_file::LocalFileTrack;
use std::collections::HashMap;
use std::fmt::Debug;
use std::time::Duration;

pub mod error;
mod local_file;

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
pub trait SourceTrait: Send + Sync + Debug {
    fn info(&self) -> SourceResult<SourceInfo>;

    fn build_source(&self) -> SourceResult<RawAudioSource>;

    fn get_unique_id(&self) -> String;

    fn display_title(&self) -> String {
        self.get_unique_id()
    }
}

macro_rules! define_source_kinds {
    (
        $f_name:ident=>$f_clz:ident
        $(,$name:ident=>$clz:ident)*

    ) => {
        #[derive(Debug)]
        pub enum SourceKinds{
            $f_name($f_clz)
            $(,$name ($clz))*          
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
    Local=>LocalFileTrack
}

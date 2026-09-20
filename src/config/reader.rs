use super::source::SourceId;

pub trait ConfigReader {
    type Error;

    fn read(&self, source_id: SourceId, input: &str) -> Result<super::Config, Self::Error>;
}

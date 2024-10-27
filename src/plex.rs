use serde::Deserialize;

#[derive(Deserialize)]
pub struct Part {
    pub key: String,
    pub file: String,
}

#[derive(Deserialize)]
pub struct Media {
    #[serde(rename = "Part")]
    pub parts: Vec<Part>,
}

#[derive(Deserialize)]
pub struct Metadata {
    #[serde(rename = "Media")]
    pub media: Vec<Media>,
}

#[derive(Deserialize)]
pub struct MediaContainer {
    #[serde(rename = "Metadata")]
    pub metadata: Vec<Metadata>,
}

#[derive(Deserialize)]
pub struct Container {
    #[serde(rename = "MediaContainer")]
    pub media_container: MediaContainer,
}

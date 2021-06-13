use async_trait::async_trait;
use snafu::Snafu;

use crate::domain::model::configuration::Configuration;
use crate::domain::model::document::Document;
use crate::domain::model::index::{Index, IndexVisibility};
use crate::domain::ports::container::ErasedContainer;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Container Creation Error: {}", source))]
    ContainerCreationError { source: Box<dyn std::error::Error> },

    #[snafu(display("Container Deletion Error: {}", source))]
    ContainerDeletionError { source: Box<dyn std::error::Error> },

    #[snafu(display("Container Search Error: {}", source))]
    ContainerSearchError { source: Box<dyn std::error::Error> },

    #[snafu(display("Index Refresh Error: {}", source))]
    IndexPublicationError { source: Box<dyn std::error::Error> },
}

#[async_trait]
pub trait Storage {
    async fn create_container(
        &self,
        config: Configuration,
    ) -> Result<Box<dyn ErasedContainer>, Error>;
}

// #[async_trait]
// impl<'a, T: ?Sized> Storage for Box<T>
// where
//     T: Storage + Send + Sync,
// {
//     async fn create_container<D>(
//         &self,
//         config: Configuration,
//     ) -> Result<Box<dyn ErasedContainer<Doc = D>>, Error>
//     where
//         D: Document,
//     {
//         (**self).create_container(config).await
//     }
// }
//
// // #[cfg_attr(test, mockall::automock)]
// #[async_trait]
// pub trait ErasedStorage {
//     async fn erased_create_container(&self, config: Configuration) -> Result<Index, Error>;
// }
//
// #[async_trait]
// impl Storage for (dyn ErasedStorage + Send + Sync) {
//     async fn create_container(&self, config: Configuration) -> Result<Index, Error> {
//         self.erased_create_container(config).await
//     }
// }
//
// #[async_trait]
// impl<T> ErasedStorage for T
// where
//     T: Storage + Send + Sync,
// {
//     async fn erased_create_container(&self, config: Configuration) -> Result<Index, Error> {
//         self.create_container(config).await
//     }
// }

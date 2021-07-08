use async_trait::async_trait;
use futures::stream::Stream;
use snafu::Snafu;

use crate::domain::model::{
    configuration::Configuration,
    document::Document,
    index::{Index, IndexVisibility},
    stats::InsertStats,
};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Container Creation Error: {}", source))]
    ContainerCreationError { source: Box<dyn std::error::Error> },

    #[snafu(display("Container Deletion Error: {}", source))]
    ContainerDeletionError { source: Box<dyn std::error::Error> },

    #[snafu(display("Container Search Error: {}", source))]
    ContainerSearchError { source: Box<dyn std::error::Error> },

    #[snafu(display("Document Insertion Error: {}", source))]
    DocumentInsertionError { source: Box<dyn std::error::Error> },

    #[snafu(display("Index Refresh Error: {}", source))]
    IndexPublicationError { source: Box<dyn std::error::Error> },
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Storage {
    async fn create_container(&self, config: Configuration) -> Result<Index, Error>;

    async fn delete_container(&self, index: String) -> Result<(), Error>;

    async fn find_container(&self, index: String) -> Result<Option<Index>, Error>;

    async fn insert_documents<S>(&self, index: String, documents: S) -> Result<InsertStats, Error>
    where
        S: Stream<Item = Box<dyn Document + Send + Sync + 'static>> + Send + Sync + Unpin + 'static;

    async fn publish_index(&self, index: Index, visibility: IndexVisibility) -> Result<(), Error>;
}

#[async_trait]
impl<'a, T: ?Sized> Storage for Box<T>
where
    T: Storage + Send + Sync,
{
    async fn create_container(&self, config: Configuration) -> Result<Index, Error> {
        (**self).create_container(config).await
    }

    async fn delete_container(&self, index: String) -> Result<(), Error> {
        (**self).delete_container(index).await
    }

    async fn find_container(&self, index: String) -> Result<Option<Index>, Error> {
        (**self).find_container(index).await
    }

    async fn insert_documents<S>(&self, index: String, documents: S) -> Result<InsertStats, Error>
    where
        S: Stream<Item = Box<dyn Document + Send + Sync + 'static>> + Send + Sync + Unpin + 'static,
    {
        (**self).insert_documents(index, documents).await
    }

    async fn publish_index(&self, index: Index, visibility: IndexVisibility) -> Result<(), Error> {
        (**self).publish_index(index, visibility).await
    }
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ErasedStorage {
    async fn erased_create_container(&self, config: Configuration) -> Result<Index, Error>;

    async fn erased_delete_container(&self, index: String) -> Result<(), Error>;

    async fn erased_find_container(&self, index: String) -> Result<Option<Index>, Error>;

    async fn erased_insert_documents(
        &self,
        index: String,
        documents: Box<
            dyn Stream<Item = Box<dyn Document + Send + Sync + 'static>>
                + Send
                + Sync
                + Unpin
                + 'static,
        >,
    ) -> Result<InsertStats, Error>;

    async fn erased_publish_index(
        &self,
        index: Index,
        visibility: IndexVisibility,
    ) -> Result<(), Error>;
}

#[async_trait]
impl Storage for (dyn ErasedStorage + Send + Sync) {
    async fn create_container(&self, config: Configuration) -> Result<Index, Error> {
        self.erased_create_container(config).await
    }

    async fn delete_container(&self, index: String) -> Result<(), Error> {
        self.erased_delete_container(index).await
    }

    async fn find_container(&self, index: String) -> Result<Option<Index>, Error> {
        self.erased_find_container(index).await
    }

    async fn insert_documents<S>(&self, index: String, documents: S) -> Result<InsertStats, Error>
    where
        S: Stream<Item = Box<dyn Document + Send + Sync + 'static>> + Send + Sync + Unpin + 'static,
    {
        // FIXME This is a potentially costly operation...
        self.erased_insert_documents(index, Box::new(documents))
            .await
    }

    async fn publish_index(&self, index: Index, visibility: IndexVisibility) -> Result<(), Error> {
        self.erased_publish_index(index, visibility).await
    }
}

#[async_trait]
impl<T> ErasedStorage for T
where
    T: Storage + Send + Sync,
{
    async fn erased_create_container(&self, config: Configuration) -> Result<Index, Error> {
        self.create_container(config).await
    }

    async fn erased_delete_container(&self, index: String) -> Result<(), Error> {
        self.delete_container(index).await
    }

    async fn erased_find_container(&self, index: String) -> Result<Option<Index>, Error> {
        self.find_container(index).await
    }

    async fn erased_insert_documents(
        &self,
        index: String,
        documents: Box<
            dyn Stream<Item = Box<dyn Document + Send + Sync + 'static>>
                + Send
                + Sync
                + Unpin
                + 'static,
        >,
    ) -> Result<InsertStats, Error> {
        self.insert_documents(index, documents).await
    }

    async fn erased_publish_index(
        &self,
        index: Index,
        visibility: IndexVisibility,
    ) -> Result<(), Error> {
        self.publish_index(index, visibility).await
    }
}
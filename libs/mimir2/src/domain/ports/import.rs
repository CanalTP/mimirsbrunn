use async_trait::async_trait;
use futures::stream::Stream;
use snafu::Snafu;

#[cfg(test)]
use crate::domain::model::document::tests::Book;

use crate::domain::model::configuration::Configuration;
use crate::domain::model::document::Document;
use crate::domain::model::index::{Index, IndexVisibility};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Index Creation Error: {}", source))]
    // #[snafu(visibility(pub))]
    IndexCreation { source: Box<dyn std::error::Error> },

    #[snafu(display("Index Publication Error: {}", source))]
    // #[snafu(visibility(pub))]
    IndexPublication { source: Box<dyn std::error::Error> },

    #[snafu(display("Storage Connection Error: {}", source))]
    // #[snafu(visibility(pub))]
    StorageConnection { source: Box<dyn std::error::Error> },

    #[snafu(display("Document Stream Insertion Error: {}", source))]
    // #[snafu(visibility(pub))]
    DocumentStreamInsertion { source: Box<dyn std::error::Error> },

    #[snafu(display("Expected Index: {}", index))]
    ExpectedIndex { index: String },
}

/// Create index and stores documents in them.
#[async_trait]
pub trait Import {
    /// Type of document
    type Doc: Document + 'static;

    /// creates an index using the given configuration, and stores the documents.
    async fn generate_index<S>(
        &self,
        mut docs: S,
        config: Configuration,
        visibility: IndexVisibility,
    ) -> Result<Index<Self::Doc>, Error>
    where
        S: Stream<Item = Self::Doc> + Send + Sync + Unpin + 'static;
}

#[async_trait]
impl<'a, T: ?Sized> Import for Box<T>
where
    T: Import + Send + Sync,
{
    type Doc = T::Doc;
    async fn generate_index<S>(
        &self,
        docs: S,
        config: Configuration,
        visibility: IndexVisibility,
    ) -> Result<Index<Self::Doc>, Error>
    where
        S: Stream<Item = Self::Doc> + Send + Sync + Unpin + 'static,
    {
        (**self).generate_index(docs, config, visibility).await
    }
}

/// Allows building safe trait objects for Import
#[cfg_attr(test, mockall::automock(type Doc=Book;))]
#[async_trait]
pub trait ErasedImport {
    type Doc: Document;
    async fn erased_generate_index(
        &self,
        docs: Box<dyn Stream<Item = Self::Doc> + Send + Sync + Unpin + 'static>,
        config: Configuration,
        visibility: IndexVisibility,
    ) -> Result<Index<Self::Doc>, Error>;
}

#[async_trait]
impl<T: Document + 'static> Import for (dyn ErasedImport<Doc = T> + Send + Sync) {
    type Doc = T;
    async fn generate_index<S>(
        &self,
        docs: S,
        config: Configuration,
        visibility: IndexVisibility,
    ) -> Result<Index<Self::Doc>, Error>
    where
        S: Stream<Item = Self::Doc> + Send + Sync + Unpin + 'static,
    {
        self.erased_generate_index(Box::new(docs), config, visibility)
            .await
    }
}

#[async_trait]
impl<T> ErasedImport for T
where
    T: Import + Send + Sync,
{
    type Doc = T::Doc;
    async fn erased_generate_index(
        &self,
        docs: Box<dyn Stream<Item = Self::Doc> + Send + Sync + Unpin + 'static>,
        config: Configuration,
        visibility: IndexVisibility,
    ) -> Result<Index<Self::Doc>, Error> {
        self.generate_index(docs, config, visibility).await
    }
}

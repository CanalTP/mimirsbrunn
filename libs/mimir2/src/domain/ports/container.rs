use async_trait::async_trait;
use erased_serde::Serialize as ErasedSerialize;
use futures::stream::{Stream, StreamExt};
use serde::Serialize;
use snafu::Snafu;

use crate::domain::model::configuration::Configuration;
use crate::domain::model::index::{Index, IndexVisibility};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Document Insertion Error: {}", source))]
    DocumentInsertionError { source: Box<dyn std::error::Error> },
}

#[async_trait]
pub trait Container {
    type Doc: Serialize + Send + Sync + 'static;
    async fn insert_documents<S>(&self, index: String, documents: S) -> Result<usize, Error>
    where
        S: Stream<Item = Self::Doc> + Send + Sync + Unpin + 'static;
}

#[async_trait]
impl<'a, T: ?Sized, D: Serialize + Send + Sync + 'static> Container for Box<T>
where
    T: Container<Doc = D> + Send + Sync,
{
    type Doc = D;
    async fn insert_documents<S>(&self, index: String, documents: S) -> Result<usize, Error>
    where
        S: Stream<Item = Self::Doc> + Send + Sync + Unpin + 'static,
    {
        (**self).insert_documents(index, documents).await
    }
}

// #[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ErasedContainer {
    async fn erased_insert_documents(
        &self,
        index: String,
        documents: Box<
            dyn Stream<Item = Box<dyn ErasedSerialize + Send + Sync>>
                + Send
                + Sync
                + Unpin
                + 'static,
        >,
    ) -> Result<usize, Error>;
}

#[async_trait]
impl<D> Container for (dyn ErasedContainer + Send + Sync) {
    type Doc = D;
    async fn insert_documents<S>(&self, index: String, documents: S) -> Result<usize, Error>
    where
        S: Stream<Item = Self::Doc> + Send + Sync + Unpin + 'static,
    {
        // FIXME This is a potentially costly operation...
        // let documents =
        //     documents.map(|d| Box::new(d) as Box<dyn ErasedSerialize + Send + Sync + 'static>);
        self.erased_insert_documents(index, Box::new(documents))
            .await
    }
}

#[async_trait]
impl<T, D> ErasedContainer for T
where
    T: Container<Doc = D> + Send + Sync,
    D: Serialize + Send + Sync + 'static,
{
    async fn erased_insert_documents(
        &self,
        index: String,
        documents: Box<
            dyn Stream<Item = Box<dyn ErasedSerialize + Send + Sync>>
                + Send
                + Sync
                + Unpin
                + 'static,
        >,
    ) -> Result<usize, Error> {
        self.insert_documents(index, documents).await
    }
}

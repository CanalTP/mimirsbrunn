use async_trait::async_trait;
use futures::stream::Stream;
use serde::Serialize;
use std::marker::PhantomData;

use crate::domain::ports::container::{Container, Error as ContainerError};
use crate::domain::ports::storage::ErasedStorage;

#[derive(Debug, Clone)]
pub enum IndexStatus {
    Available,
    NotAvailable,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IndexVisibility {
    Private,
    Public,
}

impl Default for IndexVisibility {
    fn default() -> Self {
        IndexVisibility::Public
    }
}

pub struct Index<D> {
    pub name: String,
    pub dataset: String,
    pub doc_type: String,
    pub docs_count: u32,
    pub status: IndexStatus,
    pub phantom: PhantomData<D>,
    pub storage: Box<dyn ErasedStorage + Send + Sync + 'static>,
}

#[async_trait]
impl<D: Serialize + Send + Sync + 'static> Container for Index<D> {
    type Doc = D;

    async fn insert_documents<S>(
        &self,
        index: String,
        documents: S,
    ) -> Result<usize, ContainerError>
    where
        S: Stream<Item = Self::Doc> + Send + Sync + Unpin + 'static,
    {
        // call self.storage.insert_documents...
        unimplemented!()
    }
}

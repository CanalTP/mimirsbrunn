use async_trait::async_trait;
use erased_serde::Serialize as ErasedSerialize;
use futures::stream::Stream;
use std::marker::PhantomData;

// use crate::domain::model::configuration::Configuration;
// use crate::domain::model::document::Document;
// use crate::domain::model::index::{Index, IndexVisibility};
// use crate::domain::ports::import::{Error as ImportError, Import};
use crate::domain::ports::query::{Error as QueryError, Query};
use crate::domain::usecases::{Error as UseCaseError, UseCase};

pub struct ListDocuments<T> {
    pub query: Box<dyn Query + Send + Sync + 'static>,
    phantom: PhantomData<T>,
}

impl<T> ListDocuments<T> {
    pub fn new(query: Box<dyn Query + Send + Sync + 'static>) -> Self {
        ListDocuments {
            query,
            phantom: PhantomData,
        }
    }
}

pub struct ListDocumentsParameters {
    pub name: String,
}

#[async_trait]
impl<T> UseCase for ListDocuments<T> {
    type Res = Box<dyn Stream<Item = T> + Send + Sync + 'static>;
    type Param = ListDocumentsParameters;

    async fn execute(&self, param: Self::Param) -> Result<Self::Res, UseCaseError> {
        self.list_documents(param.name)
            .map_err(|err| UseCaseError::Execution {
                details: format!("Could not retrieve documents: {}", err.to_string()),
            })
    }
}

impl<T> Query for ListDocuments<T> {
    fn list_documents(
        &self,
        index: String,
    ) -> Result<Box<dyn Stream<Item = T> + Send + Sync + 'static>, QueryError> {
        self.query
            .list_documents(index)
            .map_err(|err| QueryError::DocumentRetrievalError {
                source: Box::new(err),
            })
    }
}

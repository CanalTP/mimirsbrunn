use async_trait::async_trait;
use futures::stream::Stream;
use std::marker::PhantomData;

use crate::domain::model::configuration::Configuration;
use crate::domain::model::document::Document;
use crate::domain::model::index::{Index, IndexVisibility};
use crate::domain::ports::import::{Error as ImportError, Import};
use crate::domain::ports::storage::ErasedStorage;
use crate::domain::ports::storage::Storage;
use crate::domain::usecases::{Error as UseCaseError, UseCase};

pub struct GenerateIndex<T> {
    pub storage: Box<dyn ErasedStorage + Send + Sync + 'static>,
    pub doc_type: PhantomData<T>,
}

impl<T> GenerateIndex<T> {
    pub fn new(storage: Box<dyn ErasedStorage + Send + Sync + 'static>) -> Self {
        GenerateIndex {
            storage,
            doc_type: PhantomData,
        }
    }
}

pub struct GenerateIndexParameters<T: Document + Send + Sync + 'static> {
    pub config: Configuration,
    pub documents: Box<dyn Stream<Item = T> + Send + Sync + Unpin + 'static>,
    pub visibility: IndexVisibility,
}

#[async_trait]
impl<T: Document + Send + Sync + 'static> UseCase for GenerateIndex<T> {
    type Res = Index;
    type Param = GenerateIndexParameters<T>;

    async fn execute(&self, param: Self::Param) -> Result<Self::Res, UseCaseError> {
        self.generate_index(param.documents, param.config, param.visibility)
            .await
            .map_err(|err| UseCaseError::Execution {
                details: format!("Could not create container: {}", err.to_string()),
            })
    }
}

#[async_trait]
impl<T: Document + Send + Sync + 'static> Import for GenerateIndex<T> {
    type Doc = T;

    async fn generate_index<S>(
        &self,
        documents: S,
        config: Configuration,
        visibility: IndexVisibility,
    ) -> Result<Index, ImportError>
    where
        S: Stream<Item = Self::Doc> + Send + Sync + Unpin + 'static,
    {
        // 1. We modify the name of the index:
        //   currently set to the dataset, it should be something like root_doc_type_dataset_timestamp
        // 2. Then we create the index
        // 3. We insert the document stream in that newly created index
        // 4. FIXME Not implemented Publish the index
        // 5. We search for the newly created index to return it.
        let config =
            config
                .normalize_index_name(T::DOC_TYPE)
                .map_err(|err| ImportError::IndexCreation {
                    source: Box::new(err),
                })?;

        let index = self.storage.create_container(config).await.map_err(|err| {
            ImportError::IndexCreation {
                source: Box::new(err),
            }
        })?;

        // self.storage
        //     .insert_documents(index.name.clone(), documents)
        //     .await
        //     .map_err(|err| ImportError::DocumentStreamInsertion {
        //         source: Box::new(err),
        //     })?;

        self.storage
            .publish_container(index.clone(), visibility)
            .await
            .map_err(|err| ImportError::IndexPublication {
                source: Box::new(err),
            })?;

        self.storage
            .find_container(index.name.clone())
            .await
            .map_err(|err| ImportError::DocumentStreamInsertion {
                source: Box::new(err),
            })?
            .ok_or(ImportError::ExpectedIndex {
                index: index.name.clone(),
            })
    }
}

#[cfg(test)]
pub mod tests {

    use serde::Serialize;

    use super::{GenerateIndex, GenerateIndexParameters};
    use crate::domain::model::configuration::Configuration;
    use crate::domain::model::document::Document;
    use crate::domain::model::index::{Index, IndexStatus, IndexVisibility};
    use crate::domain::ports::storage::MockErasedStorage;
    use crate::domain::usecases::UseCase;

    #[derive(Serialize)]
    struct TestObj {
        value: String,
    }

    impl Document for TestObj {
        const IS_GEO_DATA: bool = false;
        const DOC_TYPE: &'static str = "test-obj";

        fn id(&self) -> String {
            self.value.clone()
        }
    }

    #[tokio::test]
    async fn should_get_index_from_configuration_and_documents_stream() {
        let mut storage = MockErasedStorage::new();
        let index = Index {
            name: String::from("test"),
            doc_type: String::from("obj"),
            dataset: String::from("x"),
            status: IndexStatus::NotAvailable,
            docs_count: 0,
        };
        let index_cl = index.clone();
        storage
            .expect_erased_create_container()
            .times(1)
            .return_once(move |_| Ok(index));
        storage
            .expect_erased_insert_documents()
            .times(1)
            .return_once(move |_, _| Ok(1));
        storage
            .expect_erased_find_container()
            .times(1)
            .return_once(move |_| Ok(Some(index_cl)));
        storage
            .expect_erased_publish_index()
            .times(1)
            .return_once(move |_, _| Ok(()));
        let usecase = GenerateIndex::new(Box::new(storage));

        let config = Configuration {
            value: String::from(
                r#"{ "name": "test-index", "parameters": { "timeout": "10s", "wait_for_active_shards": "1" }, "settings": { "value": "NA" }, "mappings": { "value": "NA" } }"#,
            ),
        };

        let stream = futures::stream::iter(vec![TestObj {
            value: String::from("value"),
        }]);

        let param = GenerateIndexParameters {
            config,
            documents: Box::new(stream),
            visibility: IndexVisibility::Public,
        };

        let result = usecase.execute(param).await;
        assert!(result.is_ok());
    }
}

#![feature(test)]

extern crate test;

use docker_wrapper::*;
use tools::{BragiHandler, ElasticSearchWrapper};

#[bench]
fn bench_new(b: &mut test::Bencher) {
    let docker_wrapper = DockerWrapper::new().unwrap();
    let es_wrapper = ElasticSearchWrapper::new(&docker_wrapper);
    let mut bragi = BragiHandler::new(format!("{}/munin", es_wrapper.host()));
    b.iter(|| {
        let _response = bragi
            .get("/autocomplete?q=Parking vélo Saint-Martin&pt_dataset[]=dataset1&type[]=poi");
    });
}

Bragi uses Elasticsearch Query DSL to formulate queries to Elasticsearch. The DSL is a JSON based language.

The original mimir library makes use of `rs-es`'s types to create such a query. For example,

```
// filter to handle PT coverages
// we either want:
// * to get objects with no coverage at all (non-PT objects)
// * or the objects with coverage matching the ones we're allowed to get
fn build_coverage_condition(pt_datasets: &[&str]) -> Query {
    Query::build_bool()
        .with_should(vec![
            Query::build_bool()
                .with_must_not(Query::build_exists("coverages").build())
                .build(),
            Query::build_terms("coverages")
                .with_values(pt_datasets)
                .build(),
        ])
        .build()
}
```

This is heavily relying on the builder pattern, which is natural, considering all the options available.

I came to question the use of such types. Why not simply use something like

```
fn build_coverage_condition(pt_datasets: &[&str]) -> String {
  format!(r#"{
    "bool": {
      "should": [
        "bool": {
          "must_not": {
            "exists": {
              "field": "coverages"
            }
          }
        },
        "terms": {
          "coverages": [ xxx ]
        }
      ]
    }
  }"#);
}
```

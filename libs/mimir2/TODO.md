


## Modify ports

The same trait covers `create_container` and `insert_documents`.... What should happen instead is that there is a trait
to manage containers, lets call it `Store`, which has the following behavior:

Store // manages containers
create_container
delete_container
publish_container

The create_container returns an object implementing the trait `Container`:

Container
insert_documents
list_documents // returns all the documents in the container (maybe include paging support)

There may be a third trait, `Query`, which enables searching across multiple containers:

Query
search_documents

This `Query` trait, could possibly be merged with `Store`, as it is also cross-cutting containers, but it does not
manage containers, it just searches them...

Possibly a 4th trait, which is ContainerId

ContainerId
normalize_id

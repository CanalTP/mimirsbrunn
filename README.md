[![travis](https://travis-ci.org/CanalTP/mimirsbrunn.svg?branch=master)](https://travis-ci.org/CanalTP/mimirsbrunn)
[![GitHub license](https://img.shields.io/github/license/CanalTP/mimirsbrunn.svg)](https://github.com/CanalTP/mimirsbrunn/blob/master/LICENSE)
[![GitHub tag](https://img.shields.io/github/tag/CanalTP/mimirsbrunn.svg)](https://github.com/CanalTP/mimirsbrunn/tag)

# Mimirsbrunn

Mimirsbrunn (also called Mimir) is an independent geocoding and reverse-geocoding system written in [Rust](https://www.rust-lang.org/en-US/), and built upon [Elasticsearch](https://www.elastic.co).
It can handle addresses, streets, points-of-interest (POI), administrative regions or public transport stops.

## what's a geocoder ?

Usually [geocoding](https://en.wikipedia.org/wiki/Geocoding) refers to "the process of transforming a physical address description to a location on the Earth's surface". 
However Mimir is more a [geoparser](https://en.wikipedia.org/wiki/Toponym_resolution#Geoparsing) than a geocoder since it can resolve any ambiguous toponym to its correct location.

In other words, a geocoder reads a description (possibly incomplete) of a location, and returns a list of candidate locations (latitude / longitude) matching the input. 

Geocoding is traditionally used for autocompleting search fields used in geographic applications. For example, here is a screenshot of Qwant Maps, where the user enters a search string 20 rue hec mal, and mimir returns possible candidates in a dropdown box.

![qwant maps](https://user-images.githubusercontent.com/3987698/56976025-53ed1180-6b72-11e9-9c81-9718e92061ce.png)

## who uses it ?

* [Navitia](https://github.com/CanalTP/navitia)
* [Qwant Maps](https://www.qwant.com/maps)

If you use it too, feel free to open a pull request, we'll be happy to add your project here!

## ressources

* [A french presentation of Mimirsbrunn](https://github.com/TeXitoi/pinot2017bano/blob/master/pinot2017bano.pdf)

# How to use

## API

Mimirsbrunn exposes a [REST](https://en.wikipedia.org/wiki/Representational_state_transfer) Json api (with [bragi](https://github.com/CanalTP/mimirsbrunn/tree/master/libs/bragi)).

This API provices several services. All are documented in [OpenAPI format](https://www.openapis.org/) and can be found [documentation/openapi.yml](./documentation/openapi.yml).
All endpoints, their parameters and responses are documented in there.

OpenAPI yml files can be used in many tools and documentation environments, including swagger-ui, postman, insomnia [and many more](https://apis.guru/awesome-openapi3/).

Our openapi documentation is a work-in-progress, so feel free to suggest improvements to that openapi.yml documentation.

### Autocomplete

| feature              | route            |                                                                                        |
| -------------------- | ---------------- | --                                                                                     |
| geocoding            | `/autocomplete`  | Full text search. Tunable with optional filters, prioroties and pagination parameters. |
| reverse geocoding    | `/reverse`       | Find place closest to a lcoation at certain coordinates                                |
| Detail on one object | `/features/{id}` | Get details for a single object.                                                       |

### Monitoring API

| feature            | route      |                              |
| ------------------ | ---------- | --                           |
| status             | `/status`  | Simple JSON status object    |
| Prometheus metrics | `/metrics` | Metrics in prometheus format |


## handled datasets

Mimirsbrunn relies on geographical datasets to find what users are looking for.
These locations belong to different data types and come from various sources.
To import these locations Mimirsbrunn comes along with the following specific tools:

|       Data Types       |                     Data Sources                      |            Import Tools           |
| :--------------------: | :---------------------------------------------------: | :-------------------------------: |
|       Addresses        | OpenAddresses  or BANO (the french opendata dataset)  | openaddresses2mimir or bano2mimir |
|        Streets         |                     OpenStreetMap                     |             osm2mimir             |
|          POI           |                     OpenStreetMap                     |             osm2mimir             |
| Public Transport Stops | Navitia.io data platform  or any GTFS data repository |     ntfs2mimir or stops2mimir     |
| Administrative Regions |              OpenStreetMap or Cosmogony               |   osm2mimir or cosmogony2mimir    |

To use another datasource you have to write your own data importer.
See for instance [Fafnir](https://github.com/QwantResearch/fafnir), an external component to import POIs from another database.

For more detail about the different ways to import those data sources, check the [components documentation](https://github.com/CanalTP/mimirsbrunn/blob/master/documentation/components.md).

# Install

## with docker

[docker_mimir](https://github.com/QwantResearch/docker_mimir) is a repository with some python script to easily import some data in mimir.

## debian packages

[Kisio Digital](http://www.kisiodigital.com/), the company behind
[navitia](https://www.navitia.io/) has some available debian
8 [packages](https://ci.navitia.io/view/mimir/job/mimirsbrunn_release_package/lastSuccessfulBuild/artifact/mimirsbrunn_1.8.0_amd64.deb).

If you need some packages for a different target, you can use CanalTP's
[script](https://github.com/CanalTP/mimirsbrunn/blob/master/build_packages.sh) or
[cargo-deb](https://github.com/mmstick/cargo-deb)

## manually

### prerequiste
* [install rust](https://rustup.rs/)
* install ES: the supported ES version is 2.x (yes it's old...).
You can install it either directly on you system, or use docker.
For a disposable ES, you can run:

`docker run --name es2 -d -p '9200:9200' elasticsearch:2`

### build

#### Binaries

If you want to build local binaries, use `cargo build --release`. You may need to adjust the
version of the rust compiler prior to executing this command (eg `rustup update`).

#### Docker

If you want to build a docker image, you can use the `docker` command, with the following caveat:
You need to specify the version of debian it will be based on, as well as the version of rust.
You also want to verify that the version of Debian you specify is supported! Check the content of
`docker/Dockerfile_bragi` for that. Your `docker` command will look like:

```
docker build --build-arg DEBIAN_VERSION=buster --build-arg RUST_VERSION=1.47 ...
```

Alternatively, there is a Makefile that can assist you with several tasks. See `make help` for
details. For example `make snapshot` will build an image and push it to a repository. The
repository, as well as the build environments are specified in `deploy.env`.

### test

`cargo test`

Integration tests are spawning one ElasticSearch docker, so you'll need a recent docker version.
Only one docker is spawn, so the ES db is cleaned before each test.

# More documentation

For more precise documentation on use, troubleshooting, development please check the [documentation
directory](documentation/readme.md).

# Contributing

The project is [Licensed as AGPL 3](https://github.com/CanalTP/mimirsbrunn/blob/master/LICENSE).

We'll be happy to review all you [pull
requests](https://help.github.com/en/articles/about-pull-requests).

You can also open some issues if you find some bugs, or if you find the geocoder results not to
your liking.

Another way to contribute to this project (and to lots of others geocoders) is to add some geocoder
test on [geocoder-tester](https://github.com/geocoders/geocoder-tester), a great non regression
tool used by many geocoder. It's quite easy, you just add some new test cases with some searches
and the results that you expect.

# presentation of the other alternatives
* [pelias](https://github.com/pelias/pelias)
* [photon](https://github.com/komoot/photon)
* [addok](https://github.com/addok/addok)

TODO: add a bit more detail on all the projects

All those projects use quite the same APIs, and you can compare their results using
[geocoder-tester](https://github.com/geocoders/geocoder-tester).

For a more visual comparison, you can also use [a comparator](https://github.com/CanalTP/autocomplete-comparator).

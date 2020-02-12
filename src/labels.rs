/// for the moment we keep the old way of formating the labels, but this could change
/// the current format is '{nice name} ({city})'
/// the {nice name} being for addresses the housenumber and the street (correctly ordered)
/// and for the rest of the objects, only their names
use slog_scope::warn;

fn format_label<'a>(
    nice_name: String,
    admins: impl Iterator<Item = &'a mimir::Admin> + Clone,
    _country_codes: &[String], // Note: for the moment the country code is not used, but this could change
) -> String {
    let city = admins.clone().find(|adm| adm.is_city());
    let city_name = city.map(|a| a.name.to_string());

    match city_name {
        Some(n) => format!("{} ({})", nice_name, n),
        None => nice_name.to_string(),
    }
}

// format a label for a given lang
// Note: 2 distinct localisation information are needed for the label formatting
// and it might be worth it to clarify this a bit:
// * `country codes` [Where the place is]: (which is not yet used in this function, but could be)
//      Used on how to format the label in order to find it when you're in the place's country
// * `lang` [Who searches for the place]: lang of the user
//      Used to use the names in the user's language
fn format_i18n_label<'a>(
    nice_name: &str,
    mut admins: impl Iterator<Item = &'a mimir::Admin> + Clone,
    _country_codes: &[String], // Note: for the moment the country code is not used, but this could change
    lang: &str,
) -> String {
    admins.find(|adm| adm.is_city()).map_or_else(
        || nice_name.to_string(),
        |adm| {
            let local_admin_name = &adm.names.get(lang).unwrap_or(&adm.name);
            format!("{} ({})", nice_name, local_admin_name)
        },
    )
}

// Note: even if most of the format methods are the same for the moment,
// I feel it's better to split them to make them easier to update

/// format a label for a Street
pub fn format_street_label<'a>(
    name: &str,
    admins: impl Iterator<Item = &'a mimir::Admin> + Clone,
    country_codes: &[String],
) -> String {
    format_label(name.to_owned(), admins, country_codes)
}

/// format a label for a Poi
pub fn format_poi_label<'a>(
    name: &str,
    admins: impl Iterator<Item = &'a mimir::Admin> + Clone,
    country_codes: &[String],
) -> String {
    format_label(name.to_owned(), admins, country_codes)
}

/// format a label for a Stop
pub fn format_stop_label<'a>(
    name: &str,
    admins: impl Iterator<Item = &'a mimir::Admin> + Clone,
    country_codes: &[String],
) -> String {
    format_label(name.to_owned(), admins, country_codes)
}

/// format a name and a label for an Address
pub fn format_addr_name_and_label<'a>(
    house_number: &str,
    street_name: &str,
    admins: impl Iterator<Item = &'a mimir::Admin> + Clone,
    country_codes: &[String],
) -> (String, String) {
    let place = FormatPlaceHolder::from_addr(house_number.to_owned(), street_name.to_owned());
    let nice_name = get_short_addr_label(place, admins.clone(), country_codes)
        .unwrap_or_else(|| default_name(house_number, street_name));

    (
        nice_name.clone(),
        format_label(nice_name, admins, country_codes),
    )
}

/// create some international label for a poi
/// One label is created for each lang in the `langs` parameter
pub fn format_international_poi_label<'a>(
    poi_names: &mimir::I18nProperties,
    default_poi_name: &str,
    default_poi_label: &str,
    admins: impl Iterator<Item = &'a mimir::Admin> + Clone,
    country_codes: &[String],
    langs: &[String],
) -> mimir::I18nProperties {
    let labels = langs
        .iter()
        .filter_map(|ref lang| {
            let local_poi_name = poi_names.get(lang).unwrap_or(default_poi_name);
            let i18n_poi_label =
                format_i18n_label(local_poi_name, admins.clone(), country_codes, lang);

            if i18n_poi_label == default_poi_label {
                None
            } else {
                Some(mimir::Property {
                    key: lang.to_string(),
                    value: i18n_poi_label,
                })
            }
        })
        .collect();
    mimir::I18nProperties(labels)
}

fn default_name(house_number: &str, street: &str) -> String {
    //default formating is "{street} {hn}" as it's the most common format (but not correct for france)
    format!("{street} {hn}", street = street, hn = house_number)
}

fn get_short_addr_label<'a>(
    place: FormatPlaceHolder,
    admins: impl Iterator<Item = &'a mimir::Admin> + Clone,
    country_codes: &[String],
) -> Option<String> {
    let country_code = country_codes.iter().next().map(|c| c.to_string()); // we arbitrarily take the first country code
    address_formatter::FORMATTER
        .short_addr_format_with_config(
            place.into_place(admins),
            address_formatter::Configuration {
                country_code,
                ..Default::default()
            },
        )
        .map_err(|e| warn!("impossible to format label: {}", e))
        .ok()
}

struct FormatPlaceHolder {
    street: String,
    house_number: Option<String>,
}

impl FormatPlaceHolder {
    pub fn from_addr(house_number: String, street: String) -> Self {
        Self {
            street,
            house_number: Some(house_number),
        }
    }

    pub fn into_place<'b>(
        self,
        admins: impl Iterator<Item = &'b mimir::Admin>,
    ) -> address_formatter::Place {
        use address_formatter::Component;
        let mut place = address_formatter::Place::default();
        place[Component::HouseNumber] = self.house_number;
        place[Component::Road] = Some(self.street);

        for a in admins {
            if let Some(addr_equivalent) = cosmo_to_addr_formatter_type(&a.zone_type) {
                place[addr_equivalent] = Some(a.name.clone());
            }
        }
        place
    }
}

fn cosmo_to_addr_formatter_type(
    cosmo_type: &Option<cosmogony::ZoneType>,
) -> Option<address_formatter::Component> {
    use address_formatter::Component;
    match cosmo_type {
        Some(cosmogony::ZoneType::City) => Some(Component::City),
        Some(cosmogony::ZoneType::Country) => Some(Component::Country),
        Some(cosmogony::ZoneType::State) => Some(Component::State),
        Some(cosmogony::ZoneType::Suburb) => Some(Component::Suburb),
        // not sure, but it seems a cosmogony::StateDistrict is a County in address_formatter
        Some(cosmogony::ZoneType::StateDistrict) => Some(Component::County),
        _ => None,
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cosmogony::ZoneType;

    fn make_i18_prop(val: &[(&str, &str)]) -> mimir::I18nProperties {
        val.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn get_nl_admins() -> Vec<mimir::Admin> {
        vec![
            mimir::Admin {
                id: "admin:amsterdam".to_string(),
                level: 10,
                name: "Amsterdam".to_string(),
                names: make_i18_prop(&[
                    ("ja", "アムステルダム"),
                    ("ru", "Амстердам"),
                ]),
                label: "Amsterdam, Noord-Hollad, Nederland".to_string(),
                labels: make_i18_prop(&[
                    ("ja", "アムステルダム, Noord-Holland, オランダ"),
                    ("it", "Amsterdam, Noord-Holland, Paesi Bassi"),
                    ("fr", "Amsterdam, Hollande-Septentrionale, Pays-Bas"),
                    (
                        "ru",
                        "Амстердам, Северная Голландия, Нидерланды",
                    ),
                ]),
                zone_type: Some(ZoneType::City),
                ..Default::default()
            },
            mimir::Admin {
                id: "admin:noord-holland".to_string(),
                level: 4,
                name: "Noordh-Holland".to_string(),
                label: "Noord-Hollad, Nederland".to_string(),
                zone_type: Some(ZoneType::State),
                ..Default::default()
            },
            mimir::Admin {
                id: "admin:Nederland".to_string(),
                level: 2,
                name: "Nederland".to_string(),
                label: "Nederland".to_string(),
                zone_type: Some(ZoneType::Country),
                ..Default::default()
            },
        ]
    }

    fn get_fr_admins() -> Vec<mimir::Admin> {
        vec![
            mimir::Admin {
                id: "admin:paris".to_string(),
                level: 8,
                name: "Paris".to_string(),
                label: "Paris (75000-75116), Île-de-France, France".to_string(),
                zone_type: Some(ZoneType::City),
                ..Default::default()
            },
            mimir::Admin {
                id: "admin:idf".to_string(),
                level: 4,
                name: "Île-de-France".to_string(),
                label: "Île-de-France, France".to_string(),
                zone_type: Some(ZoneType::State),
                ..Default::default()
            },
            mimir::Admin {
                id: "admin:france".to_string(),
                level: 2,
                name: "France".to_string(),
                label: "France".to_string(),
                zone_type: Some(ZoneType::Country),
                ..Default::default()
            },
        ]
    }

    #[test]
    fn nl_addr() {
        let (name, label) = format_addr_name_and_label(
            "573",
            "Herengracht",
            get_nl_admins().iter(),
            &vec!["nl".to_owned()],
        );
        assert_eq!(name, "Herengracht 573");
        assert_eq!(label, "Herengracht 573 (Amsterdam)");
    }
    #[test]
    fn nl_street() {
        let label = format_street_label(
            "Herengracht",
            get_nl_admins().iter(),
            &vec!["nl".to_owned()],
        );
        assert_eq!(label, "Herengracht (Amsterdam)");
    }
    #[test]
    fn nl_poi() {
        let label = format_poi_label(
            "Delirium Cafe",
            get_nl_admins().iter(),
            &vec!["nl".to_owned()],
        );
        assert_eq!(label, "Delirium Cafe (Amsterdam)");
    }

    #[test]
    fn fr_addr() {
        let (name, label) = format_addr_name_and_label(
            "20",
            "rue hector malot",
            get_fr_admins().iter(),
            &vec!["fr".to_owned()],
        );
        assert_eq!(name, "20 rue hector malot");
        assert_eq!(label, "20 rue hector malot (Paris)");
    }
    #[test]
    fn fr_street() {
        let label = format_street_label(
            "rue hector malot",
            get_fr_admins().iter(),
            &vec!["fr".to_owned()],
        );
        assert_eq!(label, "rue hector malot (Paris)");
    }
    #[test]
    fn fr_poi() {
        let label = format_poi_label("Le Rossli", get_fr_admins().iter(), &vec!["fr".to_owned()]);
        assert_eq!(label, "Le Rossli (Paris)");
    }

    #[test]
    fn nl_poi_in_russian() {
        // searching for the rembrandt museum (https://www.openstreetmap.org/node/250624673) in russian
        let poi_names = make_i18_prop(&[("ru", "Дом-музей Рембрандта")]);
        let label = format_international_poi_label(
            &poi_names,
            "Rembrandthuis",
            "Rembrandthuis (Amsterdam)",
            get_nl_admins().iter(),
            &vec!["nl".to_owned()],
            &["ru".to_owned()],
        );
        assert_eq!(
            label,
            make_i18_prop(&[(
                "ru",
                "Дом-музей Рембрандта (Амстердам)",
            ),])
        );
    }

    #[test]
    fn nl_poi_in_french() {
        // searching for the rembrandt museum (https://www.openstreetmap.org/node/250624673) in french
        // since the poi has no french name, the default one is used (and thus is not returned)
        let poi_names = make_i18_prop(&[("ru", "Дом-музей Рембрандта")]);
        let label = format_international_poi_label(
            &poi_names,
            "Rembrandthuis",
            "Rembrandthuis (Amsterdam)",
            get_nl_admins().iter(),
            &vec!["nl".to_owned()],
            &["fr".to_owned()],
        );
        assert_eq!(label, make_i18_prop(&[]));
    }

    #[test]
    fn nl_poi_in_japanese() {
        // searching for the rembrandt museum (https://www.openstreetmap.org/node/250624673) in japanane
        // since the poi has no japanese name, the default one is used, but we use the translated japanse name of Amsterdam
        let poi_names = make_i18_prop(&[("ru", "Дом-музей Рембрандта")]);
        let label = format_international_poi_label(
            &poi_names,
            "Rembrandthuis",
            "Rembrandthuis (Amsterdam)",
            get_nl_admins().iter(),
            &vec!["nl".to_owned()],
            &["ja".to_owned()],
        );
        assert_eq!(
            label,
            make_i18_prop(&[("ja", "Rembrandthuis (アムステルダム)"),])
        );
    }
}

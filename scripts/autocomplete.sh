#!/bin/sh

query="$*"
bragi_endpoint="http://localhost:4000/autocomplete"
pt_dataset="fr"
input_type=""
limit=10

curl_cmd="curl -s --data-urlencode \"q=${query}\""
  [[ ! -z "${pt_dataset}" ]] && curl_cmd="${curl_cmd} --data-urlencode pt_dataset[]=${pt_dataset}"
  [[ ! -z "${input_type}" ]] && curl_cmd="${curl_cmd} --data-urlencode type[]=${input_type}"
curl_cmd="${curl_cmd} --data-urlencode limit=${limit}"
curl_cmd="${curl_cmd} --data-urlencode _debug=true"
curl_cmd="${curl_cmd} --data-urlencode request_id=test"
curl_cmd="${curl_cmd} --get ${bragi_endpoint}"

#
# # This is an alternative that uses a shape to constrain the results to a geographic area.
# # curl_cmd="curl -s -d @idf.geojson -X POST \"http://localhost:4000/autocomplete?q=stade&pt_dataset[]=stif&_debug=true\" --header \"Content-Type:application/json\""
#
# echo "${curl_cmd}"
resp=$(eval ${curl_cmd})
# echo "${resp}"
echo "${resp}" | jq '[ .features[] | { "label": .properties.geocoding.label, "type": .properties.geocoding.type, "zone_type": .properties.geocoding.zone_type, "level": .properties.geocoding.level } ]'

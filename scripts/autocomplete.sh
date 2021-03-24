#!/bin/sh

# https://stackoverflow.com/questions/296536/how-to-urlencode-data-for-curl-command
rawurlencode() {
  local string="${1}"
  local strlen=${#string}
  local encoded=""
  local pos c o

  for (( pos=0 ; pos<strlen ; pos++ )); do
     c=${string:$pos:1}
     case "$c" in
        [-_.~a-zA-Z0-9] ) o="${c}" ;;
        * )               printf -v o '%%%02x' "'$c"
     esac
     encoded+="${o}"
  done
  echo "${encoded}"    # You can either set a return variable (FASTER) 
  REPLY="${encoded}"   #+or echo the result (EASIER)... or both... :p
}

query=`rawurlencode "$*"`
bragi_endpoint="http://localhost:4000/autocomplete"
# geojson_shape=""
# scopes=""
geojson_shape="arcueil.geojson"
scopes="poi,stop"
datasets="fr"
types=""
limit=10

${IFS+"false"} && unset oldifs || oldifs="$IFS"   # backup IFS

if [[ ! -z "${geojson_shape}" ]]; then
  curl_cmd="curl -s --data @${geojson_shape} --header \"Content-Type:application/json\" -X POST"

  curl_cmd="${curl_cmd} \"${bragi_endpoint}?q=${query}"

  if [[ ! -z "${scopes}" ]]; then
    IFS=','
    read -ra scopes_array <<< "$scopes"
    for scope in "${scopes_array[@]}"; do
      curl_cmd="${curl_cmd}&shape_scope[]=${scope}"
    done
  fi
else
  curl_cmd="curl -s -X GET"
  curl_cmd="${curl_cmd} \"${bragi_endpoint}?q=${query}"
fi

if [[ ! -z "${datasets}" ]]; then
  IFS=','
  read -ra datasets_array <<< "$datasets"
  for dataset in "${datasets_array[@]}"; do
    curl_cmd="${curl_cmd}&pt_dataset[]=${dataset}"
  done
fi

if [[ ! -z "${types}" ]]; then
  IFS=','
  read -ra types_array <<< "$types"
  for typ in "${types_array[@]}"; do
    curl_cmd="${curl_cmd}&type[]=${typ}"
  done
fi

curl_cmd="${curl_cmd}&limit=${limit}"
curl_cmd="${curl_cmd}&_debug=true\""

${oldifs+"false"} && unset IFS || IFS="$oldifs"    # restore IFS.

# echo "${curl_cmd}"
resp=$(eval ${curl_cmd})
# echo "${resp}"
echo "${resp}" | jq '[ .features[] | { "label": .properties.geocoding.label, "type": .properties.geocoding.type, "zone_type": .properties.geocoding.zone_type, "level": .properties.geocoding.level } ]'
# echo "${resp}" | jq '[ .hits.hits[] | { "label": ._source.label, "type": ._source.type } ]'

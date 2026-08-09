#!/usr/bin/env bash

# Находит пользователей AD, у которых userPrincipalName не оканчивается
# на заданный суффикс (включая пользователей без этого атрибута).
# По умолчанию используется Kerberos-аутентификация текущего ticket cache.

set -euo pipefail

usage() {
    cat >&2 <<'EOF'
Usage:
  find_invalid_upn.sh [options]

Options:
  --url URL       LDAP URL (по умолчанию: $LDAP_URL)
  --base DN       база поиска (по умолчанию: $LDAP_SEARCH_BASE или
                  $LDAP_AUTH_SEARCH_BASE_DN)
  --suffix VALUE  ожидаемый суффикс (по умолчанию: @main.sgu.ru)
  --output FILE   JSON-файл результата (по умолчанию: invalid_upn_users.json)
  --simple        использовать простой bind вместо Kerberos GSSAPI;
                  нужны LDAP_BIND_DN и LDAP_BIND_PASSWORD
  -h, --help      показать эту справку

Пример:
  kinit admin@MAIN.SGU.RU
  LDAP_URL=ldap://dc.main.sgu.ru:389 \
  LDAP_AUTH_SEARCH_BASE_DN='DC=main,DC=sgu,DC=ru' \
  ./scripts/find_invalid_upn.sh
EOF
}

url="ldap://lama.main.sgu.ru:389"
base="OU=КНиИТ,OU=Факультеты,DC=main,DC=sgu,DC=ru"
suffix="@main.sgu.ru"
output="invalid_upn_users.json"
simple_bind=false

while (($# > 0)); do
    case "$1" in
        --url)
            (($# >= 2)) || { echo "--url требует значение" >&2; exit 2; }
            url="$2"; shift 2
            ;;
        --base)
            (($# >= 2)) || { echo "--base требует значение" >&2; exit 2; }
            base="$2"; shift 2
            ;;
        --suffix)
            (($# >= 2)) || { echo "--suffix требует значение" >&2; exit 2; }
            suffix="$2"; shift 2
            ;;
        --output)
            (($# >= 2)) || { echo "--output требует значение" >&2; exit 2; }
            output="$2"; shift 2
            ;;
        --simple)
            simple_bind=true; shift
            ;;
        -h|--help)
            usage; exit 0
            ;;
        *)
            echo "Неизвестный аргумент: $1" >&2
            usage
            exit 2
            ;;
    esac
done

command -v ldapsearch >/dev/null || {
    echo "Ошибка: не найден ldapsearch (пакет openldap-clients)." >&2
    exit 1
}
[[ -n "$url" ]] || { echo "Задайте LDAP_URL или передайте --url." >&2; exit 2; }
[[ -n "$base" ]] || {
    echo "Задайте LDAP_SEARCH_BASE или LDAP_AUTH_SEARCH_BASE_DN (либо --base)." >&2
    exit 2
}
[[ -n "$suffix" ]] || { echo "Суффикс не может быть пустым." >&2; exit 2; }
[[ -n "$output" ]] || { echo "Имя JSON-файла не может быть пустым." >&2; exit 2; }

# Экранируем спецсимволы LDAP filter из значения, заданного пользователем.
escaped_suffix=$(printf '%s' "$suffix" | sed 's/\\/\\5c/g; s/\*/\\2a/g; s/(/\\28/g; s/)/\\29/g; s/\\x00/\\00/g')
filter="(&(objectCategory=person)(objectClass=user)(!(userPrincipalName=*${escaped_suffix})))"
result_file=$(mktemp)
trap 'rm -f "$result_file"' EXIT

# -N запрещает reverse-DNS canonicalization имени для SASL. Иначе alias
# lama.main.sgu.ru может быть заменён на другое имя, для которого нет SPN.
ldap_args=(-N -o ldif-wrap=no -H "$url" -b "$base" "$filter" \
    sAMAccountName userPrincipalName distinguishedName)
if [[ "$simple_bind" == true ]]; then
    [[ -n "${LDAP_BIND_DN:-}" && -n "${LDAP_BIND_PASSWORD:-}" ]] || {
        echo "Для --simple нужны LDAP_BIND_DN и LDAP_BIND_PASSWORD." >&2
        exit 2
    }
    ldap_args+=(-x -D "$LDAP_BIND_DN" -w "$LDAP_BIND_PASSWORD")
else
    if ! find /usr/lib /usr/lib64 -path '*/sasl2/libgssapiv2.so*' -print -quit 2>/dev/null | grep -q .; then
        echo "Ошибка: SASL-плагин GSSAPI не найден." >&2
        echo "Установите пакет cyrus-sasl-gssapi либо запустите с --simple." >&2
        exit 1
    fi
    ldap_args+=(-Y GSSAPI)
fi

if ! ldapsearch "${ldap_args[@]}" >"$result_file"; then
    echo "Ошибка: LDAP-поиск не выполнен." >&2
    exit 1
fi

awk -v output="$output" '
    function json_escape(value) {
        gsub(/\\/, "\\\\", value)
        gsub(/"/, "\\\"", value)
        gsub(/\t/, "\\t", value)
        gsub(/\r/, "\\r", value)
        gsub(/\n/, "\\n", value)
        return value
    }
    function flush(   value, separator) {
        if (dn == "" && sam == "" && upn == "") return
        value = upn == "" ? "<отсутствует>" : upn
        separator = count == 0 ? "" : ",\n"
        printf "%s  {\n    \"dn\": \"%s\",\n    \"sAMAccountName\": \"%s\",\n    \"userPrincipalName\": \"%s\"\n  }", \
            separator, json_escape(dn), json_escape(sam), json_escape(value) > output
        count++
        dn = sam = upn = ""
    }
    BEGIN { print "[" > output }
    /^dn:/ {
        flush()
        dn = substr($0, index($0, ": ") + 2)
        next
    }
    /^sAMAccountName: / { sam = substr($0, 17); next }
    /^userPrincipalName: / { upn = substr($0, 20); next }
    /^$/ { flush(); next }
    END {
        flush()
        print "\n]" > output
        close(output)
        printf "Найдено: %d. JSON записан в %s\n", count, output > "/dev/stderr"
    }
' "$result_file"

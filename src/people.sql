SELECT
    person.c_personid,
    person.c_name_chn,
    person.c_name,
    dynasty.c_dynasty_chn,
    dynasty.c_dynasty,
    person.c_birthyear,
    person.c_deathyear,
    person.c_by_range,
    person.c_dy_range,
    person.c_fl_earliest_year,
    person.c_fl_latest_year,
    person.c_index_year,
    native.c_name_chn,
    native.c_name,
    index_place.c_name_chn,
    index_place.c_name
FROM BIOG_MAIN AS person
LEFT JOIN DYNASTIES AS dynasty ON dynasty.c_dy = person.c_dy
LEFT JOIN ADDR_CODES AS index_place ON index_place.c_addr_id = person.c_index_addr_id
LEFT JOIN ADDR_CODES AS native ON native.c_addr_id = CASE
    WHEN person.c_index_addr_type_code = 1 AND index_place.c_addr_id > 0
        THEN index_place.c_addr_id
    ELSE (
        SELECT address.c_addr_id
        FROM BIOG_ADDR_DATA AS address
        WHERE address.c_personid = person.c_personid
            AND address.c_addr_type = 1
            AND address.c_addr_id > 0
            AND COALESCE(address.c_delete, 0) = 0
        ORDER BY address.c_sequence, address.c_addr_id
        LIMIT 1
    )
END
WHERE person.c_personid > 0
ORDER BY person.c_personid

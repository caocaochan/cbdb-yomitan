SELECT name.c_personid, name.c_alt_name_chn, name.c_alt_name,
    name.c_alt_name_type_code, code.c_name_type_desc_chn
FROM ALTNAME_DATA AS name
LEFT JOIN ALTNAME_CODES AS code ON code.c_name_type_code = name.c_alt_name_type_code
WHERE name.c_alt_name_chn IS NOT NULL
ORDER BY name.c_personid,
    CASE name.c_alt_name_type_code
        WHEN 4 THEN 0
        WHEN 5 THEN 1
        WHEN 3 THEN 2
        WHEN 8 THEN 3
        WHEN 6 THEN 4
        ELSE 5
    END,
    name.c_alt_name_type_code, name.c_sequence, name.c_alt_name_chn,
    NULLIF(TRIM(name.c_alt_name), '') IS NULL, name.c_alt_name

WITH candidates AS (
    SELECT
        posting.c_personid AS person,
        CASE posting.c_assume_office_code WHEN 1 THEN 'Office' ELSE 'Appointment' END AS kind,
        trim(office.c_office_chn) AS title,
        CASE WHEN posting.c_firstyear BETWEEN 1 AND 2200 THEN posting.c_firstyear END AS year,
        CASE
            WHEN posting.c_assume_office_code = 1 THEN 1
            WHEN posting.c_firstyear BETWEEN 1 AND 2200 THEN 2
            ELSE 6
        END AS priority,
        CASE
            WHEN posting.c_firstyear BETWEEN 1 AND 2200
                AND posting.c_lastyear BETWEEN posting.c_firstyear AND posting.c_firstyear + 60
            THEN posting.c_lastyear - posting.c_firstyear
            ELSE 0
        END AS preference
    FROM POSTED_TO_OFFICE_DATA AS posting
    JOIN OFFICE_CODES AS office ON office.c_office_id = posting.c_office_id
    WHERE coalesce(posting.c_assume_office_code, 0) IN (0, 1)

    UNION ALL

    SELECT
        entry.c_personid, 'Exam',
        CASE
            WHEN code.c_entry_desc_chn = '科舉: 進士(籠統)' THEN '進士'
            WHEN code.c_entry_desc_chn LIKE '科舉:%' THEN trim(substr(code.c_entry_desc_chn, 4))
            WHEN code.c_entry_desc_chn LIKE '科舉：%' THEN trim(substr(code.c_entry_desc_chn, 4))
            ELSE trim(code.c_entry_desc_chn)
        END,
        CASE WHEN entry.c_year BETWEEN 1 AND 2200 THEN entry.c_year END,
        3, 0
    FROM ENTRY_DATA AS entry
    JOIN ENTRY_CODES AS code ON code.c_entry_code = entry.c_entry_code
    WHERE code.c_entry_desc_chn LIKE '科舉:%' OR code.c_entry_desc_chn LIKE '科舉：%'

    UNION ALL

    SELECT
        work.c_personid, 'Work',
        CASE WHEN work.c_role_id = 3 THEN '《' || trim(text.c_title_chn) || '》 (compiler)'
             ELSE '《' || trim(text.c_title_chn) || '》' END,
        CASE WHEN work.c_year BETWEEN 1 AND 2200 THEN work.c_year END,
        4,
        CASE WHEN text.c_extant = 1 THEN 2 ELSE 0 END + (work.c_role_id = 1)
    FROM BIOG_TEXT_DATA AS work
    JOIN TEXT_CODES AS text ON text.c_textid = work.c_textid
    WHERE work.c_role_id IN (1, 3)
        AND trim(text.c_title_chn) NOT IN ('', '未詳', '未知', '不詳')
),
ranked AS (
    SELECT *, row_number() OVER (
        PARTITION BY person
        ORDER BY priority, preference DESC, year IS NULL, year DESC, title
    ) AS choice
    FROM candidates
    WHERE length(title) BETWEEN 2 AND 50
        AND title NOT IN ('未詳', '未知', '不詳', '無', '[Missing Data]', '未考', '未知官')
)
SELECT person, kind, title, year
FROM ranked
WHERE choice = 1
ORDER BY person

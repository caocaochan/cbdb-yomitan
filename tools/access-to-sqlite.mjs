import { closeSync, openSync, readFileSync, unlinkSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import MDBReader from 'mdb-reader';

// Copy the columns used by the converter from one or more standalone data MDBs.
const tables = {
    BIOG_MAIN: {
        c_personid: 'INTEGER PRIMARY KEY', c_name_chn: 'TEXT', c_name: 'TEXT',
        c_dy: 'INTEGER', c_birthyear: 'INTEGER', c_deathyear: 'INTEGER',
        c_by_range: 'INTEGER', c_dy_range: 'INTEGER', c_fl_earliest_year: 'INTEGER',
        c_fl_latest_year: 'INTEGER', c_index_year: 'INTEGER', c_index_addr_id: 'INTEGER',
        c_index_addr_type_code: 'INTEGER',
    },
    DYNASTIES: { c_dy: 'INTEGER PRIMARY KEY', c_dynasty_chn: 'TEXT', c_dynasty: 'TEXT' },
    ADDR_CODES: { c_addr_id: 'INTEGER PRIMARY KEY', c_name_chn: 'TEXT', c_name: 'TEXT' },
    BIOG_ADDR_DATA: {
        c_personid: 'INTEGER', c_addr_id: 'INTEGER', c_addr_type: 'INTEGER',
        c_sequence: 'INTEGER', c_delete: 'INTEGER',
    },
    ALTNAME_DATA: {
        c_personid: 'INTEGER', c_alt_name_chn: 'TEXT', c_alt_name: 'TEXT',
        c_alt_name_type_code: 'INTEGER', c_sequence: 'INTEGER',
    },
    ALTNAME_CODES: {
        c_name_type_code: 'INTEGER PRIMARY KEY', c_name_type_desc_chn: 'TEXT',
    },
};

const [output, ...inputs] = process.argv.slice(2);
if (!output || inputs.length === 0) {
    throw new Error('Usage: node tools/access-to-sqlite.mjs <output.sqlite3> <DATA.mdb> [...]');
}

function copyTable(db, name, types, table) {
    const sourceNames = new Map(table.getColumnNames().map(column => [column.toLowerCase(), column]));
    const columns = Object.keys(types);
    for (const column of columns) {
        if (!sourceNames.has(column) && column !== 'c_delete') {
            throw new Error(`${name} is missing ${column}`);
        }
    }
    const accessColumns = columns.map(column => sourceNames.get(column));
    const rows = table.getData({ columns: accessColumns.filter(Boolean) });
    db.exec(`CREATE TABLE ${name} (${columns.map(column => `${column} ${types[column]}`).join(', ')})`);
    const insert = db.prepare(`INSERT INTO ${name} VALUES (${columns.map(() => '?').join(', ')})`);
    for (const row of rows) {
        insert.run(...accessColumns.map(column => {
            const value = row[column] ?? null;
            return typeof value === 'boolean' ? Number(value) : value;
        }));
    }
    console.log(`${name}: ${rows.length} rows`);
}

closeSync(openSync(output, 'wx'));
let db;
try {
    db = new DatabaseSync(output);
    db.exec('BEGIN');
    const pending = new Set(Object.keys(tables));
    for (const input of inputs) {
        const reader = new MDBReader(readFileSync(input));
        for (const table of reader.getTableNames()) {
            const name = table.toUpperCase();
            if (pending.delete(name)) copyTable(db, name, tables[name], reader.getTable(table));
        }
        if (pending.size === 0) break;
    }
    if (pending.size > 0) throw new Error(`Missing Access tables: ${[...pending].join(', ')}`);
    db.exec(`
        CREATE INDEX biog_address_person ON BIOG_ADDR_DATA(c_personid, c_addr_type, c_sequence);
        CREATE INDEX altname_person ON ALTNAME_DATA(c_personid);
        COMMIT;
    `);
} catch (error) {
    db?.close();
    unlinkSync(output);
    throw error;
}
db.close();

import sqlite3

conn = sqlite3.connect("blackteaspeak.db")
cursor = conn.cursor()

# Get all tables
cursor.execute("SELECT name FROM sqlite_master WHERE type='table';")
tables = cursor.fetchall()
print("Tables in database:", tables)

for table in tables:
    table_name = table[0]
    print(f"\n--- Table: {table_name} ---")
    try:
        cursor.execute(f"PRAGMA table_info({table_name});")
        columns = cursor.fetchall()
        print("  Columns:", [col[1] for col in columns])
        
        cursor.execute(f"SELECT * FROM {table_name} LIMIT 5;")
        rows = cursor.fetchall()
        for row in rows:
            print("    ", row)
    except Exception as e:
        print("  Error:", e)

conn.close()

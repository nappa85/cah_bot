CREATE TABLE chats (id INTEGER PRIMARY KEY AUTOINCREMENT, telegram_id BIGINT NOT NULL, start_date DATETIME NOT NULL, end_date DATETIME DEFAULT NULL, players INTEGER NOT NULL DEFAULT 0, turn INTEGER NOT NULL DEFAULT 1, rando_carlissian BOOLEAN NOT NULL DEFAULT false, pick INTEGER NOT NULL DEFAULT 1);
CREATE TABLE players (id INTEGER PRIMARY KEY AUTOINCREMENT, telegram_id BIGINT NOT NULL, chat_id INTEGER NOT NULL, name VARCHAR(255) NOT NULL, turn INTEGER NOT NULL, points INTEGER NOT NULL DEFAULT 0, UNIQUE (id, chat_id));
CREATE TABLE packs (id INTEGER PRIMARY KEY AUTOINCREMENT, name VARCHAR(255) NOT NULL, official BOOLEAN NOT NULL DEFAULT false);
CREATE TABLE cards (id INTEGER PRIMARY KEY AUTOINCREMENT, pack_id INTEGER NOT NULL, color CHAR(5) NOT NULL, pick INTEGER DEFAULT NULL, text VARCHAR(255) NOT NULL);
CREATE TABLE hands (id INTEGER PRIMARY KEY AUTOINCREMENT, player_id INTEGER NOT NULL, chat_id INTEGER NOT NULL, card_id INTEGER NOT NULL, picked_on_turn INTEGER NOT NULL, played_on_turn INTEGER DEFAULT NULL, seq INTEGER NOT NULL DEFAULT 0, won BOOLEAN NOT NULL DEFAULT false);
CREATE TABLE chat_packs (chat_id INTEGER, pack_id INTEGER, PRIMARY KEY (chat_id, pack_id));
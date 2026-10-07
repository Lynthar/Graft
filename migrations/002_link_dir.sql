-- Where hard links for this client go when a torrent names its files differently.
ALTER TABLE clients ADD COLUMN link_dir TEXT;

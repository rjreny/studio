import { useEffect, useState } from "react";
import {
  importFriendUsernames,
  listFriends,
  syncFriends,
} from "../../platform/filmLibrary";
import type { FriendRow } from "../../platform/types/film";
import { Poster } from "./Poster";
import { RatingDisplay } from "./RatingDisplay";
import { getHome } from "../../platform/filmLibrary";
import { Shelf } from "./Shelf";

export function FriendsView({
  onStatus,
  onRefresh: _onRefresh,
}: {
  onStatus: (s: string) => void;
  onRefresh: () => Promise<void>;
}) {
  const [friends, setFriends] = useState<FriendRow[]>([]);
  const [bulk, setBulk] = useState("");
  const [feed, setFeed] = useState<
    { username: string; title: string; year: number | null; rating: number | null; poster: string | null }[]
  >([]);
  const [busy, setBusy] = useState(false);

  async function load() {
    const [rows, home] = await Promise.all([listFriends(), getHome()]);
    setFriends(rows);
    setFeed(
      home.friendFeed.map((e) => ({
        username: e.username,
        title: e.title,
        year: e.year,
        rating: e.rating,
        poster: e.poster,
      })),
    );
  }

  useEffect(() => {
    void load().catch(() => onStatus("Could not load friends"));
  }, [onStatus]);

  async function addBulk() {
    setBusy(true);
    try {
      const added = await importFriendUsernames(bulk);
      setBulk("");
      await load();
      onStatus(`Added ${added} friend${added === 1 ? "" : "s"}`);
    } finally {
      setBusy(false);
    }
  }

  async function refreshAll() {
    setBusy(true);
    try {
      await syncFriends();
      onStatus("Syncing friend feeds in the background");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="friends-page page-pad">
      <header className="page-head">
        <div>
          <h1>Friends</h1>
          <p className="muted">Public RSS only, newest first globally</p>
        </div>
        <button type="button" className="play-btn" disabled={busy} onClick={() => void refreshAll()}>
          Sync all
        </button>
      </header>
      <section className="solid-card friend-manager">
        <h2>Following</h2>
        <textarea
          value={bulk}
          onChange={(e) => setBulk(e.target.value)}
          placeholder="Paste Letterboxd usernames, one per line"
          rows={4}
        />
        <button type="button" className="ghost-pill" disabled={busy} onClick={() => void addBulk()}>
          Import usernames
        </button>
        <ul className="friend-list">
          {friends.map((f) => (
            <li key={f.id}>
              <strong>@{f.username}</strong>
              <span className="muted">
                {f.lastSyncAt ? `Last sync ${new Date(f.lastSyncAt).toLocaleString()}` : "Not synced"}
              </span>
              {f.lastSyncError ? <span className="form-error">{f.lastSyncError}</span> : null}
            </li>
          ))}
        </ul>
      </section>
      <Shelf title="Global feed">
        {feed.map((e, idx) => (
          <div key={`${e.username}-${idx}`} className="film-card">
            <Poster name={e.title} poster={e.poster} large />
            <strong>{e.title}</strong>
            <span className="muted">
              @{e.username}
              {e.year ? `  ${e.year}` : ""}
            </span>
            <RatingDisplay value={e.rating} compact />
          </div>
        ))}
      </Shelf>
    </div>
  );
}

import { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  getHome,
  importFriendUsernames,
  listFriends,
  removeFriend,
  syncFriends,
} from "../../platform/filmLibrary";
import type { FriendActivityItem, FriendRow } from "../../platform/types/film";
import { Poster } from "./Poster";
import { RatingDisplay } from "./RatingDisplay";

export function FriendsView({
  onStatus,
  onRefresh,
}: {
  onStatus: (s: string) => void;
  onRefresh: () => Promise<void>;
}) {
  const [friends, setFriends] = useState<FriendRow[]>([]);
  const [draft, setDraft] = useState("");
  const [feed, setFeed] = useState<FriendActivityItem[]>([]);
  const [busy, setBusy] = useState(false);

  async function load() {
    const [rows, home] = await Promise.all([listFriends(), getHome()]);
    setFriends(rows);
    setFeed(home.friendFeed);
  }

  useEffect(() => {
    void load().catch(() => onStatus("Could not load friends"));
  }, [onStatus]);

  async function addFriends() {
    if (!draft.trim()) return;
    setBusy(true);
    try {
      const added = await importFriendUsernames(draft);
      setDraft("");
      await load();
      await onRefresh();
      onStatus(`Added ${added} friend${added === 1 ? "" : "s"}`);
    } finally {
      setBusy(false);
    }
  }

  async function removeOne(friend: FriendRow) {
    const ok = await ask(
      `Stop following @${friend.username}? Their ratings will leave your feed.`,
      { title: "Remove friend?", kind: "warning", okLabel: "Remove", cancelLabel: "Cancel" },
    );
    if (!ok) return;
    setBusy(true);
    try {
      await removeFriend(friend.id);
      await load();
      await onRefresh();
      onStatus(`Removed @${friend.username}`);
    } catch {
      onStatus(`Could not remove @${friend.username}`);
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
          <p className="muted">Public Letterboxd diaries only</p>
        </div>
        <button type="button" className="play-btn" disabled={busy} onClick={() => void refreshAll()}>
          Sync all
        </button>
      </header>
      <div className="friends-layout">
        <aside className="solid-card friends-side">
          <h2>Following</h2>
          <form
            className="friend-add"
            onSubmit={(e) => {
              e.preventDefault();
              void addFriends();
            }}
          >
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              placeholder="Username, or several"
              autoCapitalize="off"
              spellCheck={false}
            />
            <button type="submit" className="ghost-pill" disabled={busy || !draft.trim()}>
              Add
            </button>
          </form>
          <ul className="friend-list">
            {friends.map((f) => (
              <li key={f.id}>
                <div>
                  <strong>@{f.username}</strong>
                  <span className="muted">
                    {f.lastSyncAt ? new Date(f.lastSyncAt).toLocaleDateString() : "Not synced"}
                  </span>
                  {f.lastSyncError ? <span className="form-error">{f.lastSyncError}</span> : null}
                </div>
                <button
                  type="button"
                  className="text-btn"
                  disabled={busy}
                  aria-label={`Remove @${f.username}`}
                  onClick={() => void removeOne(f)}
                >
                  Remove
                </button>
              </li>
            ))}
            {!friends.length ? <li className="muted">Nobody yet.</li> : null}
          </ul>
        </aside>
        <section className="solid-card friends-feed">
          <h2>Latest ratings</h2>
          <ul className="activity-list">
            {feed.map((e, idx) => (
              <li key={`${e.username}-${e.title}-${idx}`}>
                <Poster name={e.title} poster={e.poster} />
                <div className="activity-copy">
                  <strong title={e.title}>{e.title}</strong>
                  <span className="muted">
                    @{e.username}
                    {e.year ? `  ${e.year}` : ""}
                  </span>
                </div>
                <RatingDisplay value={e.rating} starsOnly />
              </li>
            ))}
          </ul>
          {!feed.length ? <p className="muted">Sync friends to fill this feed.</p> : null}
        </section>
      </div>
    </div>
  );
}

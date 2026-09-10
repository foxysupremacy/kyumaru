# Kyumaru Frontend Handoff Documentation

This specification guides the **AlmondEyeDB** frontend (`almond-eye-db-site`) in consuming player inventory data extracted by the **Kyumaru** in-game Hachimi plugin.

---

## 1. Architecture & Privacy Guarantee

```
┌────────────────────────────────────────────────────────────────────────┐
│                        In-Game (Kyumaru Plugin)                        │
├────────────────────────────────────────────────────────────────────────┤
│ 1. Hooks LZ4_decompress_safe_ext in libnative.dll                      │
│ 2. Detects load/index and captures raw MessagePack                     │
│ 3. Writes local backups:                                               │
│    • %USERPROFILE%\Documents\Kyumaru\inventory.json                    │
│    • %USERPROFILE%\Documents\Kyumaru\veterans.json                     │
│ 4. User clicks [ Open in Browser & Sync ] in Hachimi overlay:          │
│    Launches browser to https://database.almond_eye.tech/import/#data=...│
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Browser Launch (URL Hash Fragment)
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                      Browser (AlmondEyeDB Web App)                     │
├────────────────────────────────────────────────────────────────────────┤
│ 5. Reads window.location.hash (#data=<base64url_deflated_payload>)     │
│ 6. Decompresses in-memory via DecompressionStream("deflate")           │
│ 7. Stores into window.localStorage:                                    │
│    • almondeye_owned_cards: Record<string, number>                     │
│    • almondeye_owned_umas: Record<string, [number, number]>             │
│    • almondeye_last_sync: ISO string                                   │
│ 8. Strips #data from URL bar (history.replaceState)                    │
│ 9. Dispatches local storage event to update Deck Builder reactively    │
└────────────────────────────────────────────────────────────────────────┘
```

> [!IMPORTANT]
> **Zero-Server Privacy Guarantee:**
> Because the data payload is passed inside the **URL hash fragment (`#data=...`)**, browsers **never transmit this data across the internet** to Cloudflare or your Next.js server. The data is processed 100% client-side in the player's browser sandbox.

---

## 2. Ingest Methods

AlmondEyeDB supports two ingest methods:
1. **One-Click URL Sync (Recommended):** Player clicks `[ Open in Browser & Sync ]` in the in-game overlay $\rightarrow$ opens `/import/#data=...`.
2. **Local JSON File Drag & Drop:** Player drags `%USERPROFILE%\Documents\Kyumaru\inventory.json` or `veterans.json` directly onto the web app.

---

## 3. Data Schemas

### A. Compact URL Hash Payload (`#data=...`)

After Base64URL decoding and Deflate decompression, the payload is a compact JSON object:

```json
{
  "cards": {
    "10001": 4,
    "30005": 1,
    "30308": 4
  },
  "umas": {
    "100101": [3, 1],
    "112901": [3, 5]
  }
}
```

#### Field Specifications:
* **`cards`**: `Record<string, number>`
  * **Key:** `support_card_id` as string (e.g. `"10001"` matches `CardIndexEntry.id` in `lib/api.ts`).
  * **Value:** Limit break count (`0` to `4`):
    * `0`: 0 Uncaps (Base level cap: 30 for SSR, 25 for SR, 20 for R)
    * `1`: 1 Uncap (Level cap: 35 for SSR)
    * `2`: 2 Uncaps (Level cap: 40 for SSR)
    * `3`: 3 Uncaps (Level cap: 45 for SSR)
    * `4`: 4 Uncaps / MLB (Level cap: 50 for SSR, 45 for SR, 40 for R)
* **`umas`**: `Record<string, [number, number]>`
  * **Key:** Character & costume `card_id` (e.g. `"112901"`).
  * **Value:** `[rarity, talent_level]`:
    * `rarity`: 1 to 5 stars (`★`).
    * `talent_level`: Awakening level (`1` to `7`). *Note: Cygames expanded the awakening cap from Lv 5 up to Lv 7 for skill evolution.*

#### Compression & Size Benchmark (Verified with Live Production Data):
* Account: 352 support cards, 43 owned Umas.
* Raw JSON: 4,185 characters.
* Compressed URL hash: **1,260 characters** (~1.2 KB).
* Browser limit: 32,768 characters (Chrome/Edge/Firefox/Safari). The payload utilizes **< 4% of the browser limit**.

---

### B. Local File: `inventory.json` (~99 KB)

Generated at `%USERPROFILE%\Documents\Kyumaru\inventory.json`:

```json
{
  "support_card_list": [
    {
      "viewer_id": 231175073,
      "support_card_id": 10001,
      "exp": 40510,
      "limit_break_count": 4,
      "favorite_flag": 0,
      "stock": 0,
      "possess_time": "2021-04-16 02:00:14",
      "create_time": "2021-04-16 02:00:14"
    }
  ],
  "card_list": [
    {
      "card_id": 100101,
      "rarity": 3,
      "talent_level": 1,
      "create_time": "2026-08-24 18:07:23",
      "skill_data_array": []
    }
  ],
  "user_info": {
    "name": "Haruna～",
    "viewer_id": 231175073,
    "rank_score": 2779634,
    "best_team_evaluation_point": 832071,
    "support_card_id_array": [30308, 30307, 30301, 30297, 30282, 30283]
  }
}
```

---

### C. Local File: `veterans.json` (~3.1 MB)

Generated at `%USERPROFILE%\Documents\Kyumaru\veterans.json`:
Contains all 99 Hall of Fame veteran characters with all 44 attributes (Speed, Stamina, Power, Guts, Wit, 3-generation parent lineage, and complete skill arrays) for future Parent Deck & Trait Factor search.

---

## 4. TypeScript Interfaces

Add to `almond-eye-db-site/lib/kyumaru-types.ts`:

```typescript
export interface KyumaruSyncPayload {
  cards: Record<string, number>; // support_card_id -> limit_break_count (0..4)
  umas: Record<string, [number, number]>; // card_id -> [rarity, talent_level]
}

export interface KyumaruCardItem {
  viewer_id?: number;
  support_card_id: number;
  exp: number;
  limit_break_count: number;
  favorite_flag?: number;
  possess_time?: string;
}

export interface KyumaruUmaItem {
  card_id: number;
  rarity: number;
  talent_level: number;
}

export interface KyumaruUserInfo {
  name: string;
  viewer_id: number;
  rank_score?: number;
  best_team_evaluation_point?: number;
  support_card_id_array?: number[];
}

export interface KyumaruInventoryDump {
  support_card_list: KyumaruCardItem[];
  card_list: KyumaruUmaItem[];
  user_info?: KyumaruUserInfo;
}
```

---

## 5. Implementation Guide

### A. Dedicated Import Route: `app/import/page.tsx`

Create `almond-eye-db-site/app/import/page.tsx`:

```tsx
"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { type KyumaruSyncPayload } from "@/lib/kyumaru-types";

export default function ImportPage() {
  const [status, setStatus] = useState<"idle" | "loading" | "success" | "error">("idle");
  const [stats, setStats] = useState<{ cards: number; mlb: number; umas: number } | null>(null);
  const [errorMessage, setErrorMessage] = useState<string>("");

  useEffect(() => {
    async function processHash() {
      const hash = window.location.hash;
      if (!hash.startsWith("#data=")) {
        setStatus("idle");
        return;
      }

      setStatus("loading");
      const b64 = hash.slice(6);

      try {
        // 1. Base64 URL decode (handles URL-safe characters & padding)
        const unescaped = b64.replace(/-/g, "+").replace(/_/g, "/");
        const padded = unescaped.padEnd(unescaped.length + ((4 - (unescaped.length % 4)) % 4), "=");
        const binaryStr = atob(padded);
        const bytes = Uint8Array.from(binaryStr, (c) => c.charCodeAt(0));

        // 2. Native browser Deflate decompression (supported in modern Chrome, Edge, Safari, Firefox)
        const stream = new Response(
          new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate"))
        );
        const payload: KyumaruSyncPayload = await stream.json();

        if (!payload.cards && !payload.umas) {
          throw new Error("Invalid payload: missing cards or umas");
        }

        // 3. Save to localStorage
        if (payload.cards) {
          window.localStorage.setItem("almondeye_owned_cards", JSON.stringify(payload.cards));
        }
        if (payload.umas) {
          window.localStorage.setItem("almondeye_owned_umas", JSON.stringify(payload.umas));
        }
        window.localStorage.setItem("almondeye_last_sync", new Date().toISOString());

        // 4. Dispatch storage event for active tabs/windows
        window.dispatchEvent(new Event("almondeye_inventory_updated"));

        // 5. Clean up address bar (strip hash fragment)
        window.history.replaceState(null, "", window.location.pathname);

        const cardEntries = Object.entries(payload.cards || {});
        const mlbCount = cardEntries.filter(([_, lb]) => lb === 4).length;
        const umaCount = Object.keys(payload.umas || {}).length;

        setStats({
          cards: cardEntries.length,
          mlb: mlbCount,
          umas: umaCount,
        });
        setStatus("success");
      } catch (err: any) {
        console.error("Failed to unpack Kyumaru sync data:", err);
        setErrorMessage(err.message || "Failed to decompress inventory payload.");
        setStatus("error");
      }
    }

    processHash();
  }, []);

  return (
    <div className="max-w-xl mx-auto py-16 px-4 text-center">
      <h1 className="text-2xl font-bold tracking-tight mb-4">Kyumaru Game Data Import</h1>

      {status === "loading" && (
        <div className="py-12 flex flex-col items-center gap-3">
          <div className="w-8 h-8 border-4 border-emerald-500 border-t-transparent rounded-full animate-spin" />
          <p className="text-zinc-400">Unpacking inventory from game...</p>
        </div>
      )}

      {status === "success" && stats && (
        <div className="bg-emerald-500/10 border border-emerald-500/20 rounded-2xl p-8 mb-6 text-left">
          <div className="flex items-center gap-3 mb-4">
            <div className="w-10 h-10 rounded-full bg-emerald-500/20 flex items-center justify-center text-xl">
              ✓
            </div>
            <div>
              <h2 className="text-lg font-bold text-emerald-400">Inventory Synchronized!</h2>
              <p className="text-xs text-zinc-400">Saved to local browser storage.</p>
            </div>
          </div>

          <div className="grid grid-cols-3 gap-3 my-6 text-center">
            <div className="bg-black/20 rounded-xl p-3 border border-emerald-500/10">
              <div className="text-2xl font-black text-white">{stats.cards}</div>
              <div className="text-xs text-zinc-400">Owned Cards</div>
            </div>
            <div className="bg-black/20 rounded-xl p-3 border border-emerald-500/10">
              <div className="text-2xl font-black text-amber-400">{stats.mlb}</div>
              <div className="text-xs text-zinc-400">MLB Cards (4★)</div>
            </div>
            <div className="bg-black/20 rounded-xl p-3 border border-emerald-500/10">
              <div className="text-2xl font-black text-sky-400">{stats.umas}</div>
              <div className="text-xs text-zinc-400">Owned Umas</div>
            </div>
          </div>

          <div className="flex items-center justify-between pt-2">
            <Link
              href="/"
              className="inline-flex items-center px-5 py-2.5 bg-emerald-600 hover:bg-emerald-500 text-white text-sm font-semibold rounded-xl shadow-lg transition-colors"
            >
              Go to Deck Builder →
            </Link>
            <span className="text-xs text-zinc-500">Zero data sent to server</span>
          </div>
        </div>
      )}

      {status === "error" && (
        <div className="bg-red-500/10 border border-red-500/20 rounded-2xl p-6 text-left mb-6">
          <h2 className="text-lg font-bold text-red-400 mb-1">Import Failed</h2>
          <p className="text-sm text-zinc-400 mb-4">{errorMessage}</p>
          <p className="text-xs text-zinc-500">
            Ensure you clicked the button in the in-game overlay without modifying the URL.
          </p>
        </div>
      )}

      {status === "idle" && (
        <div className="bg-zinc-900/60 border border-zinc-800 rounded-2xl p-8 text-center">
          <p className="text-zinc-400 mb-2">No incoming sync payload detected.</p>
          <p className="text-xs text-zinc-500">
            Open the in-game Hachimi overlay menu inside <em>Umamusume: Pretty Derby</em> and click{" "}
            <strong>[ Open in Browser & Sync ]</strong>.
          </p>
        </div>
      )}
    </div>
  );
}
```

---

### B. Custom React Hook: `lib/use-owned-cards.ts`

Create `almond-eye-db-site/lib/use-owned-cards.ts` to consume owned cards anywhere in the app:

```typescript
"use client";

import { useState, useEffect, useCallback } from "react";

const STORAGE_KEY = "almondeye_owned_cards";

export function useOwnedCards() {
  const [ownedCards, setOwnedCards] = useState<Record<string, number>>({});

  const reload = useCallback(() => {
    if (typeof window === "undefined") return;
    try {
      const raw = window.localStorage.getItem(STORAGE_KEY);
      setOwnedCards(raw ? JSON.parse(raw) : {});
    } catch {
      setOwnedCards({});
    }
  }, []);

  useEffect(() => {
    reload();

    const handleCustomUpdate = () => reload();
    const handleStorage = (e: StorageEvent) => {
      if (e.key === STORAGE_KEY) reload();
    };

    window.addEventListener("almondeye_inventory_updated", handleCustomUpdate);
    window.addEventListener("storage", handleStorage);

    return () => {
      window.removeEventListener("almondeye_inventory_updated", handleCustomUpdate);
      window.removeEventListener("storage", handleStorage);
    };
  }, [reload]);

  const getLimitBreak = useCallback(
    (cardId: number | string): number | undefined => {
      return ownedCards[String(cardId)];
    },
    [ownedCards]
  );

  const isOwned = useCallback(
    (cardId: number | string): boolean => {
      return ownedCards[String(cardId)] !== undefined;
    },
    [ownedCards]
  );

  return {
    ownedCards,
    getLimitBreak,
    isOwned,
    totalOwned: Object.keys(ownedCards).length,
  };
}
```

---

### C. Integrating Limit Break Badges in `components/card-picker-popover.tsx`

In `almond-eye-db-site/components/card-picker-popover.tsx`:

1. Import the hook:
   ```typescript
   import { useOwnedCards } from "../lib/use-owned-cards";
   ```

2. Call the hook and add an "Owned Only" filter state:
   ```typescript
   const { getLimitBreak, isOwned } = useOwnedCards();
   const [onlyOwned, setOnlyOwned] = useState(false);
   ```

3. Filter cards:
   ```typescript
   const filteredCards = useMemo(() => {
     return allCards.filter((card) => {
       if (onlyOwned && !isOwned(card.id)) return false;
       // ... other existing search and type filters ...
       return true;
     });
   }, [allCards, onlyOwned, isOwned, ...]);
   ```

4. Render Limit Break Badges on the Card Item:
   ```tsx
   {(() => {
     const lb = getLimitBreak(card.id);
     if (lb === undefined) {
       return (
         <span className="text-[10px] px-1.5 py-0.5 rounded bg-zinc-800 text-zinc-500 font-medium">
           Unowned
         </span>
       );
     }
     if (lb === 4) {
       return (
         <span className="text-[10px] px-1.5 py-0.5 rounded bg-amber-500/20 text-amber-300 font-bold border border-amber-500/40">
           MLB
         </span>
       );
     }
     return (
       <span className="text-[10px] px-1.5 py-0.5 rounded bg-sky-500/20 text-sky-300 font-semibold border border-sky-500/30">
         {lb}★
       </span>
     );
   })()}
   ```

5. Add the "Only Owned" Toggle Button in the popover toolbar:
   ```tsx
   <button
     type="button"
     onClick={() => setOnlyOwned(!onlyOwned)}
     className={`px-2.5 py-1 text-xs rounded-lg font-medium border transition-colors ${
       onlyOwned
         ? "bg-amber-500/20 border-amber-500/40 text-amber-300"
         : "bg-zinc-800/60 border-zinc-700/60 text-zinc-400 hover:text-white"
     }`}
   >
     {onlyOwned ? "✓ Owned Only" : "Filter: All Cards"}
   </button>
   ```

---

## 6. Real Test Data Fixtures

For local testing and unit tests, use the live production dumps generated by Kyumaru:
* **Full Account Dump:** [`kyumaru/responses/inventory.json`](file:///Users/fubuki/Documents/AlmondEyeDB/kyumaru/responses/inventory.json) (352 cards, 43 umas, user info)
* **Veterans Dump:** [`kyumaru/responses/veterans.json`](file:///Users/fubuki/Documents/AlmondEyeDB/kyumaru/responses/veterans.json) (99 Hall of Fame trained characters)

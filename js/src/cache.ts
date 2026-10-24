interface CacheEntry {
    bytes: ArrayBuffer;
    size: number;
}

export class CacheStore {
    readonly budgetBytes: number;
    private readonly entries = new Map<string, CacheEntry>();
    private usedBytes = 0;

    constructor(budgetBytes: number) {
        if (!Number.isSafeInteger(budgetBytes) || budgetBytes <= 0) {
            throw new Error('cache budget must be a positive integer');
        }
        this.budgetBytes = budgetBytes;
    }

    get(key: string): ArrayBuffer | undefined {
        const entry = this.entries.get(key);
        if (!entry) return undefined;
        this.entries.delete(key);
        this.entries.set(key, entry);
        return entry.bytes;
    }

    set(key: string, bytes: ArrayBuffer): void {
        if (bytes.byteLength > this.budgetBytes) {
            throw new Error(`artifact exceeds cache budget: ${key}`);
        }
        const existing = this.entries.get(key);
        if (existing) {
            this.usedBytes -= existing.size;
            this.entries.delete(key);
        }
        while (this.usedBytes + bytes.byteLength > this.budgetBytes) {
            const oldest = this.entries.entries().next().value as [string, CacheEntry] | undefined;
            if (!oldest) break;
            this.entries.delete(oldest[0]);
            this.usedBytes -= oldest[1].size;
        }
        this.entries.set(key, { bytes, size: bytes.byteLength });
        this.usedBytes += bytes.byteLength;
    }

    clear(): void {
        this.entries.clear();
        this.usedBytes = 0;
    }

    get sizeBytes(): number {
        return this.usedBytes;
    }
}

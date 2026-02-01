import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Lock, Unlock } from "lucide-react";
import { toast } from "sonner";

// Check if we are running inside Tauri
const isTauri = () => typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__ !== undefined;


interface UnlockScreenProps {
    onUnlock: () => void;
}

export function UnlockScreen({ onUnlock }: UnlockScreenProps) {
    const [password, setPassword] = useState("");
    const [error, setError] = useState("");
    const [loading, setLoading] = useState(false);
    const [isInitialized, setIsInitialized] = useState(false);

    useEffect(() => {
        if (isTauri()) {
            checkInit();
        }
    }, []);

    const checkInit = async () => {
        if (!isTauri()) return;
        try {
            const init = await invoke<boolean>("is_wallet_initialized");
            setIsInitialized(init);
        } catch (e) {
            console.error(e);
        }
    };

    const handleUnlock = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!isTauri()) return;
        setLoading(true);
        setError("");

        try {
            await invoke("unlock_wallet", { password });
            toast.success(isInitialized ? "Wallet Unlocked" : "Wallet Created Successfully");
            onUnlock();
        } catch (err: any) {
            const msg = err.toString();
            setError(msg);
            toast.error(msg);
            setLoading(false);
        }
    };

    return (
        <div className="min-h-screen flex items-center justify-center bg-background text-foreground p-4">
            <div className="max-w-md w-full space-y-8 rounded-xl border bg-card p-10 shadow-lg">
                <div className="flex flex-col items-center gap-4">
                    <div className="p-3 rounded-full bg-primary/10 text-primary">
                        {isInitialized ? <Lock className="h-8 w-8" /> : <Unlock className="h-8 w-8" />}
                    </div>
                    <h2 className="text-2xl font-bold tracking-tight">
                        {isInitialized ? "Unlock Citadel" : "Initialize Wallet"}
                    </h2>
                    <p className="text-sm text-muted-foreground text-center">
                        {isInitialized
                            ? "Enter your secure password to access the terminal."
                            : "Set a secure password to create your new encrypted keystore."}
                    </p>
                </div>

                <form onSubmit={handleUnlock} className="space-y-6">
                    <div className="space-y-2">
                        <input
                            type="password"
                            value={password}
                            onChange={(e) => setPassword(e.target.value)}
                            placeholder="Enter Password"
                            className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                            required
                        />
                    </div>

                    {error && <div className="text-sm text-red-500 text-center">{error}</div>}

                    <Button type="submit" className="w-full" disabled={loading}>
                        {loading ? "Decrypting..." : isInitialized ? "Unlock" : "Create Wallet"}
                    </Button>
                </form>
            </div>
        </div>
    );
}

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { SentimentChart } from "@/components/SentimentChart";
import { UnlockScreen } from "@/components/UnlockScreen";
import { Shield, ShieldAlert, Coins, Activity } from "lucide-react";
import { toast } from "sonner";
import "./App.css";

// Check if we are running inside Tauri
const isTauri = () => typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__ !== undefined;

interface GovernanceState {
  total_staked: number;
  stakers_count: number;
  total_rewards: number;
  my_staked: number;
  my_pending_rewards: number;
}

interface VaultMetrics {
  tvl_btc: number;
  active_dlcs: number;
  circuit_breaker: boolean;
  spread: number;
}

function App() {
  const [gov, setGov] = useState<GovernanceState | null>(null);
  const [vault, setVault] = useState<VaultMetrics | null>(null);
  const [locked, setLocked] = useState(true);

  const checkLock = async () => {
    if (!isTauri()) return;
    try {
      const isLocked = await invoke<boolean>("is_wallet_locked");
      setLocked(isLocked);
      if (!isLocked) fetchData();
    } catch (e) {
      console.error(e);
    }
  };

  const fetchData = async () => {
    if (!isTauri()) return;
    try {
      const g = await invoke<GovernanceState>("get_governance_state");
      setGov(g);

      const v = await invoke<VaultMetrics>("get_vault_metrics");
      setVault(v);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    checkLock();
    const interval = setInterval(() => {
      if (!locked) fetchData();
    }, 5000);
    return () => clearInterval(interval);
  }, [locked]);

  const handleStake = async () => {
    try {
      await invoke("stake", { amount: 100 });
      toast.success("Successfully staked 100 CTDL");
      fetchData();
    } catch (e) {
      toast.error("Stake failed: " + e);
    }
  };

  const handleClaim = async () => {
    try {
      const amount = await invoke("claim");
      toast.success(`Claimed ${amount} satoshis!`);
      fetchData();
    } catch (e) {
      toast.error("Claim failed: " + e);
    }
  };

  if (!isTauri()) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background text-foreground p-8">
        <div className="max-w-md text-center space-y-4 p-6 border rounded-xl bg-card shadow-lg">
          <ShieldAlert className="h-12 w-12 text-yellow-500 mx-auto" />
          <h2 className="text-xl font-bold">Browser Environment Detected</h2>
          <p className="text-muted-foreground">
            The Sovereign Citadel Protocol requires the Tauri native shell to access encrypted keys and hardware.
          </p>
          <code className="block p-2 bg-muted rounded text-sm">npm run tauri dev</code>
        </div>
      </div>
    );
  }

  if (locked) {
    return <UnlockScreen onUnlock={() => { setLocked(false); fetchData(); }} />;
  }

  return (
    <div className="min-h-screen bg-background text-foreground p-8">
      <header className="mb-8 flex items-center justify-between">
        <h1 className="text-3xl font-bold tracking-tight">Sovereign Citadel Terminal</h1>
        <div className="flex items-center gap-2">
          {vault?.circuit_breaker ? (
            <div className="flex items-center gap-1 text-red-500 animate-pulse">
              <ShieldAlert className="h-4 w-4" />
              <span className="text-sm font-medium">CIRCUIT BREAKER</span>
            </div>
          ) : (
            <div className="flex items-center gap-1 text-green-500">
              <Shield className="h-4 w-4" />
              <span className="text-sm font-medium">System Secure</span>
            </div>
          )}
        </div>
      </header>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        {/* Sentiment Card */}
        <div className="lg:col-span-2 rounded-xl border bg-card text-card-foreground shadow">
          <div className="p-6 flex flex-col space-y-1.5 ">
            <h3 className="font-semibold leading-none tracking-tight">Market Sentiment</h3>
            <p className="text-sm text-muted-foreground">Real-time AI Index & Risk Analysis</p>
          </div>
          <div className="p-6 pt-0">
            <SentimentChart />
          </div>
        </div>

        {/* Governance Card */}
        <div className="rounded-xl border bg-card text-card-foreground shadow">
          <div className="p-6 flex flex-col space-y-1.5">
            <h3 className="font-semibold leading-none tracking-tight">Governance</h3>
            <p className="text-sm text-muted-foreground">Staking & Rewards</p>
          </div>
          <div className="p-6 pt-0 space-y-4">
            <div className="flex justify-between items-center">
              <span className="text-sm font-medium">My Stake</span>
              <span className="text-2xl font-bold">{gov?.my_staked || 0} CTDL</span>
            </div>
            <div className="flex justify-between items-center">
              <span className="text-sm font-medium">Pending Rewards</span>
              <span className="text-xl font-bold text-green-500">
                {gov?.my_pending_rewards || 0} sats
              </span>
            </div>
            <div className="pt-4 flex gap-2">
              <Button onClick={handleStake} className="w-full">Stake 100</Button>
              <Button onClick={handleClaim} variant="outline" className="w-full">Claim</Button>
            </div>
            <div className="pt-4 border-t text-xs text-muted-foreground">
              Total Staked: {gov?.total_staked || 0} CTDL <br />
              Stakers: {gov?.stakers_count || 0}
            </div>
          </div>
        </div>

        {/* Vault & Metrics Card */}
        <div className="lg:col-span-3 grid grid-cols-1 md:grid-cols-3 gap-6">
          <div className="rounded-xl border bg-card text-card-foreground shadow p-6">
            <div className="flex flex-row items-center justify-between space-y-0 pb-2">
              <h3 className="tracking-tight text-sm font-medium">Total Value Locked</h3>
              <Coins className="h-4 w-4 text-muted-foreground" />
            </div>
            <div className="text-2xl font-bold">{vault?.tvl_btc.toFixed(4) || "0.0000"} BTC</div>
            <p className="text-xs text-muted-foreground">+20.1% from last month</p>
          </div>

          <div className="rounded-xl border bg-card text-card-foreground shadow p-6">
            <div className="flex flex-row items-center justify-between space-y-0 pb-2">
              <h3 className="tracking-tight text-sm font-medium">Active DLCs</h3>
              <Activity className="h-4 w-4 text-muted-foreground" />
            </div>
            <div className="text-2xl font-bold">{vault?.active_dlcs || 0}</div>
            <p className="text-xs text-muted-foreground">Current open positions</p>
          </div>

          <div className="rounded-xl border bg-card text-card-foreground shadow p-6">
            <div className="flex flex-row items-center justify-between space-y-0 pb-2">
              <h3 className="tracking-tight text-sm font-medium">Current Spread</h3>
              <Shield className="h-4 w-4 text-muted-foreground" />
            </div>
            <div className="text-2xl font-bold">{(vault?.spread || 0) * 100}%</div>
            <p className="text-xs text-muted-foreground">Dynamic adjustment active</p>
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;

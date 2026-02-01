import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { SentimentChart } from "@/components/SentimentChart";
import "./App.css";

interface GovernanceState {
  total_staked: number;
  stakers_count: number;
  total_rewards: number;
  my_staked: number;
  my_pending_rewards: number;
}

function App() {
  const [gov, setGov] = useState<GovernanceState | null>(null);

  const fetchData = async () => {
    try {
      const g = await invoke<GovernanceState>("get_governance_state");
      setGov(g);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 5000);
    return () => clearInterval(interval);
  }, []);

  const handleStake = async () => {
    try {
      await invoke("stake", { amount: 100 });
      fetchData();
    } catch (e) {
      alert("Stake failed: " + e);
    }
  };

  const handleClaim = async () => {
    try {
      await invoke("claim");
      fetchData();
      alert("Rewards claimed!");
    } catch (e) {
      alert("Claim failed: " + e);
    }
  };

  return (
    <div className="min-h-screen bg-background text-foreground p-8">
      <header className="mb-8 flex items-center justify-between">
        <h1 className="text-3xl font-bold tracking-tight">Sovereign Citadel Terminal</h1>
        <div className="flex items-center gap-2">
          <div className="h-3 w-3 rounded-full bg-green-500 animate-pulse"></div>
          <span className="text-sm text-muted-foreground">System Online</span>
        </div>
      </header>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        {/* Sentiment Card */}
        <div className="lg:col-span-2 rounded-xl border bg-card text-card-foreground shadow">
          <div className="p-6 flex flex-col space-y-1.5 ">
            <h3 className="font-semibold leading-none tracking-tight">Market Sentiment</h3>
            <p className="text-sm text-muted-foreground">Real-time AI Index</p>
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
      </div>
    </div>
  );
}

export default App;

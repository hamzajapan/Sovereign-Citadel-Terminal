import { useEffect, useState } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, ReferenceLine } from 'recharts';
import { invoke } from "@tauri-apps/api/core";

// Define data shape from Rust
interface SentimentPoint {
    timestamp: string;
    score: number; // -1.0 to 1.0
}

export function SentimentChart() {
    const [data, setData] = useState<SentimentPoint[]>([]);

    useEffect(() => {
        const fetchData = async () => {
            try {
                const result = await invoke<SentimentPoint[]>("get_sentiment_history");
                setData(result);
            } catch (e) {
                console.error("Failed to fetch sentiment:", e);
            }
        };

        fetchData();
        // Update every 5 seconds
        const interval = setInterval(fetchData, 5000);
        return () => clearInterval(interval);
    }, []);

    return (
        <div className="h-[300px] w-full mt-4">
            <ResponsiveContainer width="100%" height="100%">
                <LineChart data={data}>
                    <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="#e5e7eb" />
                    <XAxis
                        dataKey="timestamp"
                        tick={{ fontSize: 12, fill: '#6b7280' }}
                        axisLine={false}
                        tickLine={false}
                    />
                    <YAxis
                        domain={[-1, 1]}
                        tick={{ fontSize: 12, fill: '#6b7280' }}
                        axisLine={false}
                        tickLine={false}
                    />
                    <Tooltip
                        contentStyle={{ borderRadius: '8px', border: 'none', boxShadow: '0 4px 6px -1px rgb(0 0 0 / 0.1)' }}
                    />
                    {/* Zero line (Neutral) */}
                    <ReferenceLine y={0} stroke="#9ca3af" strokeDasharray="3 3" />

                    <Line
                        type="monotone"
                        dataKey="score"
                        stroke="#2563eb"
                        strokeWidth={2}
                        dot={false}
                        activeDot={{ r: 6 }}
                    />
                </LineChart>
            </ResponsiveContainer>
        </div>
    );
}

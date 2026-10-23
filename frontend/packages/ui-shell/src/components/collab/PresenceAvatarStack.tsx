import React, { useState } from 'react';
import { useCollabStore, CollabPeer } from '../../collab/useCollabStore';
import { Users } from 'lucide-react';

export interface PresenceAvatarStackProps {
  className?: string;
}

export const PresenceAvatarStack: React.FC<PresenceAvatarStackProps> = ({ className = '' }) => {
  const peers = useCollabStore((s) => s.peers);
  const connected = useCollabStore((s) => s.connected);
  const [showTooltip, setShowTooltip] = useState(false);

  // Extract initials from user name (e.g. "Engineer-102" -> "E1", "John Doe" -> "JD")
  const getInitials = (name: string): string => {
    if (!name) return '?';
    const parts = name.trim().split(/[\s-_]+/);
    if (parts.length >= 2) {
      return (parts[0][0] + parts[1][0]).toUpperCase();
    }
    return name.slice(0, 2).toUpperCase();
  };

  const peerCount = peers.length;

  return (
    <div
      className={`relative flex items-center gap-2 select-none ${className}`}
      onMouseEnter={() => setShowTooltip(true)}
      onMouseLeave={() => setShowTooltip(false)}
      onFocus={() => setShowTooltip(true)}
      onBlur={() => setShowTooltip(false)}
      tabIndex={0}
      data-testid="presence-avatar-stack"
    >
      {/* Active User Count Badge */}
      <div
        className={`flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-mono border transition-colors ${
          connected
            ? 'bg-emerald-950/50 border-emerald-800/80 text-emerald-400'
            : 'bg-neutral-900 border-neutral-800 text-neutral-400'
        }`}
        data-testid="presence-count-badge"
      >
        <span
          className={`w-2 h-2 rounded-full ${
            connected ? 'bg-emerald-400 animate-pulse' : 'bg-neutral-500'
          }`}
        />
        <span>● {peerCount} Online</span>
      </div>

      {/* Avatar Stack (Overlapping Circles) */}
      <div className="flex -space-x-1.5 overflow-hidden py-0.5">
        {peers.slice(0, 5).map((peer: CollabPeer) => {
          const initials = getInitials(peer.user?.name || '');
          const color = peer.user?.color || 'var(--collab-blue)';

          return (
            <div
              key={peer.clientId}
              className="relative inline-flex items-center justify-center w-6 h-6 rounded-full text-[10px] font-bold text-white shadow-sm ring-2 ring-[var(--bg-base)] transition-transform hover:scale-110 hover:z-10 cursor-pointer"
              style={{ backgroundColor: color }}
              title={`${peer.user?.name || 'Peer'} (${peer.activeTab || 'Idle'})`}
              data-testid={`peer-avatar-${peer.clientId}`}
            >
              {peer.user?.avatar ? (
                <img
                  src={peer.user.avatar}
                  alt={peer.user.name}
                  className="w-full h-full rounded-full object-cover"
                />
              ) : (
                <span>{initials}</span>
              )}
            </div>
          );
        })}

        {peers.length > 5 && (
          <div
            className="inline-flex items-center justify-center w-6 h-6 rounded-full text-[10px] font-bold bg-neutral-800 text-neutral-300 ring-2 ring-[var(--bg-base)]"
            data-testid="peer-avatar-overflow"
          >
            +{peers.length - 5}
          </div>
        )}
      </div>

      {/* Hover Tooltip showing details */}
      {showTooltip && (
        <div
          className="absolute top-full right-0 mt-2 z-50 w-64 p-2.5 rounded-md bg-[var(--bg-surface)] border border-neutral-700 shadow-2xl text-xs text-neutral-200 pointer-events-none"
          data-testid="presence-tooltip"
        >
          <div className="flex items-center justify-between pb-1.5 mb-1.5 border-b border-neutral-800 font-semibold text-[11px] text-neutral-400">
            <span className="flex items-center gap-1">
              <Users className="w-3.5 h-3.5 text-cyan-400" />
              Active Collaborators
            </span>
            <span className="text-[10px] text-cyan-400">{peers.length} active</span>
          </div>

          {peers.length === 0 ? (
            <div className="py-2 text-center text-neutral-400 text-[11px]">
              No other engineers connected
            </div>
          ) : (
            <ul className="space-y-1.5 max-h-48 overflow-y-auto">
              {peers.map((peer) => (
                <li
                  key={peer.clientId}
                  className="flex items-center justify-between gap-2 p-1 rounded hover:bg-neutral-800/60"
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <div
                      className="w-3 h-3 rounded-full flex-shrink-0"
                      style={{ backgroundColor: peer.user?.color || 'var(--collab-blue)' }}
                    />
                    <span className="truncate font-medium text-neutral-200">
                      {peer.user?.name || `Client ${peer.clientId}`}
                    </span>
                  </div>
                  <span className="text-[10px] text-neutral-400 truncate max-w-[100px]">
                    {peer.activeTab || peer.activeScreen || 'Idle'}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
};

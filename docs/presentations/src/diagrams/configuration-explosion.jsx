import React, { useState, useEffect } from 'react';

export default function ConfigurationExplosion() {
  const [view, setView] = useState('problem'); // 'problem' or 'solution'
  const [yamlScroll, setYamlScroll] = useState(0);

  useEffect(() => {
    if (view === 'problem') {
      const timer = setInterval(() => {
        setYamlScroll(s => (s + 1) % 100);
      }, 100);
      return () => clearInterval(timer);
    }
  }, [view]);

  const yamlLines = [
    'apiVersion: apps/v1',
    'kind: Deployment',
    'metadata:',
    '  name: mongodb',
    '  namespace: production',
    '  labels:',
    '    app: mongodb',
    '    tier: database',
    'spec:',
    '  replicas: 1',
    '  selector:',
    '    matchLabels:',
    '      app: mongodb',
    '  template:',
    '    metadata:',
    '      labels:',
    '        app: mongodb',
    '    spec:',
    '      containers:',
    '      - name: mongodb',
    '        image: mongo:7.0',
    '        ports:',
    '        - containerPort: 27017',
    '        env:',
    '        - name: MONGO_INITDB_ROOT_USERNAME',
    '          valueFrom:',
    '            secretKeyRef:',
    '              name: mongodb-secret',
    '              key: username',
    '        - name: MONGO_INITDB_ROOT_PASSWORD',
    '          valueFrom:',
    '            secretKeyRef:',
    '              name: mongodb-secret',
    '              key: password',
    '        volumeMounts:',
    '        - name: mongodb-data',
    '          mountPath: /data/db',
    '        resources:',
    '          requests:',
    '            memory: "256Mi"',
    '            cpu: "200m"',
    '          limits:',
    '            memory: "512Mi"',
    '            cpu: "500m"',
    '      volumes:',
    '      - name: mongodb-data',
    '        persistentVolumeClaim:',
    '          claimName: mongodb-pvc',
    '---',
    'apiVersion: v1',
    'kind: Service',
    '# ... 30 more lines ...',
    '---', 
    'apiVersion: v1',
    'kind: PersistentVolumeClaim',
    '# ... 20 more lines ...',
    '---',
    'apiVersion: v1',
    'kind: Secret',
    '# ... 15 more lines ...',
  ];

  const visibleLines = yamlLines.slice(yamlScroll % 40, (yamlScroll % 40) + 20);

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">CONFIGURATION EXPLOSION</h2>
      <p className="text-zinc-500 text-sm mb-6">the problem with infrastructure-as-code</p>

      {/* Toggle */}
      <div className="flex gap-2 mb-8">
        <button
          onClick={() => setView('problem')}
          className={`px-4 py-2 rounded-lg border transition-all ${
            view === 'problem'
              ? 'border-red-500 bg-red-500/10 text-red-400'
              : 'border-zinc-700 text-zinc-500 hover:border-zinc-600'
          }`}
        >
          The Problem
        </button>
        <button
          onClick={() => setView('solution')}
          className={`px-4 py-2 rounded-lg border transition-all ${
            view === 'solution'
              ? 'border-green-500 bg-green-500/10 text-green-400'
              : 'border-zinc-700 text-zinc-500 hover:border-zinc-600'
          }`}
        >
          The Solution
        </button>
      </div>

      <div className="flex gap-12 max-w-5xl w-full items-start">
        
        {/* Problem view */}
        <div className={`flex-1 transition-all duration-500 ${view === 'problem' ? 'opacity-100' : 'opacity-20'}`}>
          <div className="text-red-400 text-xs tracking-wider mb-3">TO DEPLOY MONGODB...</div>
          
          {/* Scrolling YAML wall */}
          <div className="bg-zinc-950 rounded-lg p-4 h-80 overflow-hidden relative">
            <div className="font-mono text-xs space-y-0.5">
              {visibleLines.map((line, i) => (
                <div key={i} className="text-zinc-500 whitespace-pre">
                  {line}
                </div>
              ))}
            </div>
            
            {/* Fade overlays */}
            <div className="absolute inset-x-0 top-0 h-8 bg-gradient-to-b from-zinc-950 to-transparent" />
            <div className="absolute inset-x-0 bottom-0 h-8 bg-gradient-to-t from-zinc-950 to-transparent" />
            
            {/* Scroll indicator */}
            <div className="absolute right-2 top-1/2 -translate-y-1/2 w-1 h-20 bg-zinc-800 rounded">
              <div 
                className="w-full bg-red-400/50 rounded transition-all"
                style={{ height: '30%', marginTop: `${(yamlScroll % 70)}%` }}
              />
            </div>
          </div>

          <div className="mt-4 space-y-2">
            <div className="flex justify-between text-xs">
              <span className="text-zinc-500">deployment.yaml</span>
              <span className="text-red-400">127 lines</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-zinc-500">service.yaml</span>
              <span className="text-red-400">34 lines</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-zinc-500">pvc.yaml</span>
              <span className="text-red-400">22 lines</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-zinc-500">secret.yaml</span>
              <span className="text-red-400">18 lines</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-zinc-500">configmap.yaml</span>
              <span className="text-red-400">45 lines</span>
            </div>
            <div className="border-t border-zinc-800 pt-2 flex justify-between text-sm">
              <span className="text-zinc-400">Total</span>
              <span className="text-red-400 font-medium">246 lines of YAML</span>
            </div>
          </div>
        </div>

        {/* Solution view */}
        <div className={`flex-1 transition-all duration-500 ${view === 'solution' ? 'opacity-100' : 'opacity-20'}`}>
          <div className="text-green-400 text-xs tracking-wider mb-3">TO DEPLOY MONGODB...</div>
          
          <div className="bg-zinc-950 rounded-lg p-8 h-80 flex flex-col items-center justify-center">
            <div className="font-mono text-lg text-green-400 mb-4">
              $ garden-rake offer mongodb
            </div>
            
            <div className="text-zinc-500 text-sm text-center max-w-xs">
              That's it. Health checks, volumes, networking, restart policies — 
              all handled by the offering template.
            </div>
          </div>

          <div className="mt-4 space-y-2">
            <div className="flex justify-between text-xs">
              <span className="text-zinc-500">Command</span>
              <span className="text-green-400">1 line</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-zinc-500">Config files</span>
              <span className="text-green-400">0</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-zinc-500">Time to deploy</span>
              <span className="text-green-400">~30 seconds</span>
            </div>
            <div className="border-t border-zinc-800 pt-2 flex justify-between text-sm">
              <span className="text-zinc-400">Reduction</span>
              <span className="text-green-400 font-medium">99.6%</span>
            </div>
          </div>
        </div>
      </div>

      {/* The insight */}
      <div className="mt-8 p-4 border border-zinc-800 rounded-lg max-w-xl">
        <p className="text-amber-200/70 text-sm text-center">
          {view === 'problem'
            ? "You're not deploying MongoDB. You're describing how to deploy MongoDB, in excruciating detail, every single time."
            : "The knowledge of how to deploy MongoDB exists once, in the offering template. You just say what you want."}
        </p>
      </div>
    </div>
  );
}

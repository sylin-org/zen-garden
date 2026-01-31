import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'AWS Bridge',
  description: 'Same code runs on garden or AWS',
  category: 'Architecture',
  color: 'purple',
  order: 3
};


export default function AwsBridge() {
  const [environment, setEnvironment] = useState('garden');
  const [stage, setStage] = useState(0);

  useEffect(() => {
    const timer = setInterval(() => {
      setStage(s => (s + 1) % 4);
    }, 2000);
    return () => clearInterval(timer);
  }, []);

  // The code is always the same
  const codeLines = [
    { text: 'var s3 = new AmazonS3Client();', highlight: false },
    { text: 'await s3.PutObjectAsync(bucket, key, data);', highlight: true },
    { text: 'var url = s3.GetPreSignedURL(key);', highlight: false },
  ];

  const Arrow = ({ active }) => (
    <div className="flex flex-col items-center py-4">
      <svg width="24" height="48" viewBox="0 0 24 48">
        <path 
          d="M12 0 L12 40 M6 34 L12 40 L18 34" 
          fill="none" 
          stroke={active ? '#4ade80' : '#3f3f46'} 
          strokeWidth="2"
        />
        {active && (
          <circle r="4" fill="#4ade80">
            <animateMotion dur="0.8s" repeatCount="indefinite" path="M12 0 L12 40" />
          </circle>
        )}
      </svg>
    </div>
  );

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">AWS BRIDGE</h2>
      <p className="text-zinc-500 text-sm mb-8">same code, anywhere it runs</p>

      {/* Environment toggle */}
      <div className="flex gap-2 mb-8">
        <button
          onClick={() => setEnvironment('garden')}
          className={`px-4 py-2 rounded-lg border transition-all ${
            environment === 'garden'
              ? 'border-green-500 bg-green-500/10 text-green-400'
              : 'border-zinc-700 text-zinc-500 hover:border-zinc-600'
          }`}
        >
          🌱 Zen Garden
        </button>
        <button
          onClick={() => setEnvironment('aws')}
          className={`px-4 py-2 rounded-lg border transition-all ${
            environment === 'aws'
              ? 'border-amber-500 bg-amber-500/10 text-amber-400'
              : 'border-zinc-700 text-zinc-500 hover:border-zinc-600'
          }`}
        >
          ☁️ AWS
        </button>
      </div>

      <div className="flex gap-16 items-start max-w-4xl">
        
        {/* The Application Code - SAME for both */}
        <div className="flex-1">
          <div className="text-center mb-3">
            <span className="text-zinc-500 text-xs tracking-wider">YOUR APPLICATION</span>
          </div>
          
          <div className="border border-zinc-700 rounded-lg p-4 bg-zinc-800/50">
            <div className="text-blue-400 text-xs mb-3">// Exactly the same code</div>
            <div className="font-mono text-sm space-y-1">
              {codeLines.map((line, i) => (
                <div 
                  key={i} 
                  className={line.highlight && stage >= 1 
                    ? 'text-green-400' 
                    : 'text-zinc-400'
                  }
                >
                  {line.text}
                </div>
              ))}
            </div>
          </div>
          
          <Arrow active={stage >= 1} />
          
          {/* Bridge layer */}
          <div className={`
            border rounded-lg p-3 text-center transition-all duration-500
            ${environment === 'garden' 
              ? 'border-green-500/50 bg-green-500/5' 
              : 'border-amber-500/50 bg-amber-500/5'}
          `}>
            <div className={`text-xs mb-1 ${
              environment === 'garden' ? 'text-green-400' : 'text-amber-400'
            }`}>
              ZEN GARDEN DRIVER
            </div>
            <div className="text-zinc-500 text-xs">
              {environment === 'garden' 
                ? 'detects local garden → routes to MinIO' 
                : 'detects AWS → routes to S3'}
            </div>
          </div>
          
          <Arrow active={stage >= 2} />
        </div>

        {/* The Backend - DIFFERENT based on environment */}
        <div className="flex-1">
          <div className="text-center mb-3">
            <span className="text-zinc-500 text-xs tracking-wider">
              {environment === 'garden' ? 'LOCAL BACKEND' : 'CLOUD BACKEND'}
            </span>
          </div>
          
          {environment === 'garden' ? (
            <div className="space-y-3">
              <div className="border border-green-500/50 rounded-lg p-4 bg-green-500/5">
                <div className="flex items-center gap-3 mb-2">
                  <div className="w-3 h-3 rounded-full bg-green-400" />
                  <span className="text-green-400 text-sm font-medium">MinIO</span>
                </div>
                <div className="text-zinc-500 text-xs">S3-compatible storage</div>
                <div className="text-zinc-600 text-xs mt-1">on stone-coral</div>
              </div>
              
              <div className="flex items-center gap-2 text-zinc-600 text-xs justify-center">
                <span>💾</span>
                <span>Data stays on your network</span>
              </div>
              
              <div className="text-center">
                <span className="text-green-400 text-sm">$0/month</span>
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="border border-amber-500/50 rounded-lg p-4 bg-amber-500/5">
                <div className="flex items-center gap-3 mb-2">
                  <div className="w-3 h-3 rounded-full bg-amber-400" />
                  <span className="text-amber-400 text-sm font-medium">Amazon S3</span>
                </div>
                <div className="text-zinc-500 text-xs">us-east-1</div>
                <div className="text-zinc-600 text-xs mt-1">+ CloudFront, IAM, etc.</div>
              </div>
              
              <div className="flex items-center gap-2 text-zinc-600 text-xs justify-center">
                <span>☁️</span>
                <span>Data in AWS region</span>
              </div>
              
              <div className="text-center">
                <span className="text-amber-400 text-sm">$23+/month</span>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* The insight */}
      <div className="mt-8 p-4 border border-zinc-800 rounded-lg max-w-xl">
        <div className="text-center space-y-2">
          <div className="text-zinc-300 text-sm">
            {environment === 'garden' 
              ? "Develop locally with real storage. No mocks. No LocalStack license."
              : "Deploy to AWS with zero code changes. Same SDK. Same calls."}
          </div>
          <div className="text-amber-200/70 text-xs">
            The bridge detects the environment. Your code doesn't care.
          </div>
        </div>
      </div>

      {/* What's bridged */}
      <div className="mt-6 flex gap-4">
        {[
          { aws: 'S3', local: 'MinIO' },
          { aws: 'SQS', local: 'Redis' },
          { aws: 'DynamoDB', local: 'MongoDB' },
          { aws: 'Secrets Manager', local: 'Local vault' },
        ].map((pair, i) => (
          <div key={i} className="text-center">
            <div className="text-amber-400 text-xs">{pair.aws}</div>
            <div className="text-zinc-600 text-xs">↓</div>
            <div className="text-green-400 text-xs">{pair.local}</div>
          </div>
        ))}
      </div>

      {/* Stage indicators */}
      <div className="flex gap-2 mt-6">
        {[0,1,2,3].map(i => (
          <div 
            key={i}
            className={`w-2 h-2 rounded-full transition-colors ${
              stage === i ? 'bg-amber-400' : 'bg-zinc-700'
            }`}
          />
        ))}
      </div>

      <button 
        onClick={() => setStage(0)}
        className="mt-4 text-zinc-700 text-xs hover:text-zinc-500 transition-colors"
      >
        reset
      </button>
    </div>
  );
}

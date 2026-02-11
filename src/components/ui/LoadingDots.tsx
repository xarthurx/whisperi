export const LoadingDots = () => (
  <>
    <style>{`
      @keyframes loading-bar {
        0%, 100% { transform: scaleY(1); opacity: 0.5; }
        50% { transform: scaleY(2); opacity: 1; }
      }
    `}</style>
    <div style={{ display: "flex", alignItems: "center", height: 14, gap: 2 }}>
      {[0, 1, 2].map((i) => (
        <div
          key={i}
          className="bg-foreground"
          style={{
            width: 3.5,
            height: 6,
            borderRadius: 2,
            transformOrigin: "center",
            animation: `loading-bar 0.8s ease-in-out ${i * 0.15}s infinite`,
          }}
        />
      ))}
    </div>
  </>
);

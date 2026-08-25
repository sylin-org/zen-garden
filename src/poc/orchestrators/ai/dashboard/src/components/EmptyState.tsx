interface Props {
  icon?: string;
  title: string;
  description?: string;
  action?: { label: string; onClick: () => void };
}

export default function EmptyState({ icon, title, description, action }: Props) {
  return (
    <div className="flex flex-col items-center justify-center h-full py-12 px-6 text-center">
      {icon && <div className="text-3xl mb-3 text-text-dimmer">{icon}</div>}
      <div className="text-sm font-medium text-text-dim mb-1">{title}</div>
      {description && (
        <div className="text-[11px] text-text-dimmer max-w-[300px]">{description}</div>
      )}
      {action && (
        <button
          onClick={action.onClick}
          className="mt-3 px-4 py-1.5 bg-accent hover:bg-accent-dim text-white text-[11px]
                     font-semibold rounded-lg transition-colors"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}

// A styled label component

// declare labelsize as either small or large

export type LabelSize = 'small' | 'large';

export type LabelColor = 'green' | 'yellow' | 'red' | 'gray';

// a map from labelcolor to css classes
const labelColorMap = {
  green: 'bg-green-faded/25 text-green',
  yellow: 'bg-yellow-faded/25 text-yellow',
  red: 'bg-red-faded/25 text-red',
  gray: 'bg-gray-faded/30 text-gray-300', //TODO: make the gray color also semi-transparent
};

const labelSizeMap = {
  small: 'text-smaller py-[0.25em] px-1',
  large: 'text-small py-1 px-2',
};

export default function Label({
  size = 'small',
  color = 'gray',
  className = '',
  children,
  ...rest
}: {
  size: LabelSize;
  color: LabelColor;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <span
      className={`h-fit select-none whitespace-nowrap rounded-full font-bold tracking-medium ${labelSizeMap[size]} ${labelColorMap[color]} ${className}`}
      {...rest}
    >
      {children}
    </span>
  );
}

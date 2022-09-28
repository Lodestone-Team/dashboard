// A styled label component

// declare labelsize as either small or large

export type LabelSize = 'small' | 'large';

export type LabelColor = 'green' | 'ochre' | 'red' | 'gray';

// a map from labelcolor to css classes
const labelColorMap = {
  green: 'bg-green-faded/25 text-green',
  ochre: 'bg-ochre-faded/25 text-ochre',
  red: 'bg-red-faded/25 text-red',
  gray: 'bg-gray-faded/30 text-gray-300', //TODO: make the gray color also semi-transparent
};

const labelSizeMap = {
  small: 'text-smaller py-[0.25em] px-1.5',
  large: 'text-base py-1.5 px-3',
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
      className={`select-none font-bold tracking-tight rounded-full h-fit ${labelSizeMap[size]} ${labelColorMap[color]} ${className}`}
      {...rest}
    >
      {children}
    </span>
  );
}

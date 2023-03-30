import ClipboardTextfield from 'components/ClipboardTextfield';
import Label from 'components/Atoms/Label';
import { updateInstance } from 'data/InstanceList';
import { LodestoneContext } from 'data/LodestoneContext';
import { useContext, useState } from 'react';
import { axiosPutSingleValue, stateToLabelColor } from 'utils/util';
import EditableTextfield from 'components/EditableTextfield';
import { useQueryClient } from '@tanstack/react-query';
import InstancePerformanceCard from 'components/Instance/InstancePerformanceCard';
import { InstanceContext } from 'data/InstanceContext';
import GameIcon from 'components/Atoms/GameIcon';
import { useGlobalSettings } from 'data/GlobalSettings';

import { useDocumentTitle } from 'usehooks-ts';
import InstancePlayerList from './InstancePlayerList';

import { Table, TableColumn, TableRow } from 'components/Table';
import { faTrashCan, faEdit, faSkull } from '@fortawesome/free-solid-svg-icons';
import { ButtonMenuProps } from 'components/ButtonMenu';

const InstanceOverview = () => {
  useDocumentTitle('Dashboard - Lodestone');
  const { core } = useContext(LodestoneContext);
  const { address } = core;
  const { selectedInstance: instance } = useContext(InstanceContext);
  const { data: globalSettings } = useGlobalSettings();
  const domain = (globalSettings?.domain ?? address) || 'localhost';
  const queryClient = useQueryClient();
  const uuid = instance?.uuid;

  if (!instance || !uuid) {
    return (
      <div
        className="relative flex h-full w-full flex-row justify-center overflow-y-auto px-4 pt-8 pb-10 @container"
        key={uuid}
      >
        <div className="flex h-fit min-h-full w-full grow flex-col items-start gap-2">
          <div className="flex min-w-0 flex-row items-center gap-4">
            <h1 className="dashboard-instance-heading truncate whitespace-pre">
              Instance not found
            </h1>
          </div>
        </div>
      </div>
    );
  }

  const labelColor = stateToLabelColor[instance.state];

  // tablist is map from GameType to tabs

  const setInstanceName = async (name: string) => {
    await axiosPutSingleValue<void>(`/instance/${uuid}/name`, name);
    updateInstance(uuid, queryClient, (oldData) => ({
      ...oldData,
      name,
    }));
  };

  const setInstanceDescription = async (description: string) => {
    await axiosPutSingleValue<void>(
      `/instance/${uuid}/description`,
      description
    );
    updateInstance(uuid, queryClient, (oldData) => ({
      ...oldData,
      description,
    }));
  };

  const columns: TableColumn[] = [
    { field: 'name', headerName: 'Name' },
    { field: 'age', headerName: 'Age' },
    { field: 'city', headerName: 'City', className: 'column-city' },
  ];

  const rows: TableRow[] = [
    { id: 1, name: 'John', age: 30, city: 'New York' },
    { id: 2, name: 'Jane', age: 25, city: 'Los Angeles' },
    { id: 3, name: 'Bob', age: 45, city: 'Chicago' },
  ];

  const columnsAnalog: TableColumn[] = [
    { field: 'make', headerName: 'Make' },
    { field: 'model', headerName: 'Model' },
    { field: 'lens', headerName: 'Lens' },
    { field: 'format', headerName: 'Format' },
    { field: 'year', headerName: 'Year' },
  ];
  
  const rowsAnalog1: TableRow[] = [
    { id: 1, make: 'Nikon', model: 'FM2', lens: '50mm f/1.8', format: '35mm', year: 1982 },
    { id: 2, make: 'Canon', model: 'AE-1', lens: '50mm f/1.4', format: '35mm', year: 1976 },
    { id: 3, make: 'Pentax', model: 'K1000', lens: '50mm f/2.0', format: '35mm', year: 1976 },
    { id: 4, make: 'Mamiya', model: 'RB67', lens: '127mm f/3.8', format: '120', year: 1970 },
    { id: 5, make: 'Hasselblad', model: '500CM', lens: '80mm f/2.8', format: '120', year: 1957 },
    { id: 6, make: 'Leica', model: 'M6', lens: '35mm f/2.0', format: '35mm', year: 1984 },
  ];
  
  const rowsAnalog2: TableRow[] = [
    { id: 1, make: 'Nikon', model: 'FM2', lens: '50mm f/1.8', format: '35mm', year: 1982 },
    { id: 2, make: 'Canon', model: 'AE-1', lens: '50mm f/1.4', format: '35mm', year: 1976 },
    { id: 3, make: 'Pentax', model: 'K1000', lens: '50mm f/2.0', format: '35mm', year: 1976 },
    { id: 4, make: 'Mamiya', model: 'RB67', lens: '127mm f/3.8', format: '120', year: 1970 },
    { id: 5, make: 'Hasselblad', model: '500CM', lens: '80mm f/2.8', format: '120', year: 1957 },
    { id: 6, make: 'Leica', model: 'M6', lens: '35mm f/2.0', format: '35mm', year: 1984 },
    { id: 7, make: 'Fuji', model: 'GW690III', lens: '90mm f/3.5', format: '120', year: 1980 },
    { id: 8, make: 'Minolta', model: 'X-700', lens: '50mm f/1.7', format: '35mm', year: 1981 },
  ];

  const menuItems1: ButtonMenuProps = {
    menuItems: [
      {
        label: 'Edit in file viewer',
        icon: faEdit,
        variant: 'text',
        intention: 'info',
        disabled: false,
        onClick: () => console.log('Button 1 clicked'),
      },
      {
        label: 'why the fuck is this justified start',
        icon: faSkull,
        variant: 'text',
        intention: 'info',
        disabled: false,
        onClick: () => console.log('Button 1 clicked'),
      },
      {
        label: 'Obliterate',
        icon: faTrashCan,
        variant: 'text',
        intention: 'danger',
        disabled: false,
        onClick: () => console.log('Button 2 clicked'),
      },
    ]
  };

  return (
    <>
      <div
        className="relative flex h-full w-full max-w-2xl flex-col justify-center @container"
        key={uuid}
      >
        {/* main content container */}
        <div className="flex w-full grow flex-col items-stretch gap-2 ">
          <div className="flex w-full min-w-0 flex-row items-center gap-4">
            <EditableTextfield
              initialText={instance.name}
              type={'heading'}
              onSubmit={setInstanceName}
              placeholder="No name"
              containerClassName="min-w-0"
            />
          </div>
          <div className="-mt-2 flex flex-row flex-wrap items-center gap-4">
            <GameIcon game_type={instance.game_type} className="h-6 w-6" />
            <Label size="large" color={labelColor}>
              {instance.state}
            </Label>
            <Label size="large" color={'blue'}>
              Version {instance.version}
            </Label>
            <Label size="large" color={'blue'}>
              Player Count {instance.player_count}/{instance.max_player_count}
            </Label>
            <Label size="large" color={'blue'}>
              <ClipboardTextfield
                text={`${domain}:${instance.port}`}
                color="blue"
                iconLeft={false}
              />
            </Label>
          </div>
          <div className="flex w-full flex-row items-center gap-2">
            <EditableTextfield
              initialText={instance.description}
              type={'description'}
              onSubmit={setInstanceDescription}
              placeholder="No description"
              containerClassName="min-w-0"
            />
          </div>
        </div>
      </div>
      <InstancePerformanceCard />
      <Table rows={rowsAnalog1} columns={columnsAnalog} menuOptions={menuItems1} />
      <InstancePlayerList />
    </>
  );
};

export default InstanceOverview;

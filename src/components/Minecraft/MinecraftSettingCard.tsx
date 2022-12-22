import SettingField from 'components/SettingField';
import { InstanceContext } from 'data/InstanceContext';
import { useInstanceManifest } from 'data/InstanceManifest';
import { useContext } from 'react';
import { parse } from 'minecraft-motd-util';
import { MOTDRender } from 'components/Atoms/MOTDRender';
import { axiosWrapper, convertUnicode, errorToString } from 'utils/util';
import Button from 'components/Atoms/Button';
import { useUserAuthorized } from 'data/UserInfo';
import { useQueryClient } from '@tanstack/react-query';

export default function MinecraftSettingCard() {
  const { selectedInstance: instance, selectInstance } = useContext(InstanceContext);
  if (!instance) throw new Error('No instance selected');
  const { data: manifest, isLoading } = useInstanceManifest(instance.uuid);
  const supportedOptions = manifest?.supported_operations
    ? manifest.supported_operations
    : [];
  const supportedSettings = manifest?.settings ? manifest.settings : [];
  const can_delete_instance = useUserAuthorized('can_delete_instance');
  const queryClient = useQueryClient();

  const commonSettings: {
    [key: string]: {
      name: string;
      type: 'toggle' | 'number' | 'text' | 'dropdown';
      options?: string[];
      description?: React.ReactNode;
      descriptionFunc?: (value: any) => React.ReactNode;
    };
  } = {
    gamemode: {
      name: 'Game Mode',
      type: 'dropdown',
      options: ['survival', 'creative', 'adventure', 'spectator'],
      descriptionFunc: (value: string) =>
        `New players will join in ${value} mode`,
    },
    difficulty: {
      name: 'Difficulty',
      type: 'dropdown',
      options: ['peaceful', 'easy', 'normal', 'hard'],
      descriptionFunc: (value: string) =>
        `The server difficulty is set to ${value}`,
    },
    'spawn-protection': {
      name: 'Spawn Protection',
      type: 'number',
      descriptionFunc: (value: number) =>
        `Players cannot build within ${value} blocks of the spawn.`,
    },
    'view-distance': {
      name: 'View Distance',
      type: 'number',
      descriptionFunc: (value: number) =>
        `Players can see ${value} chunks away.`,
    },
    hardcore: {
      name: 'Hardcore',
      type: 'toggle',
      descriptionFunc: (value: boolean) =>
        value
          ? 'Difficulty is set to hard and players are set to spectator mode when they die.'
          : 'No special hardcore settings are enabled.',
    },
    pvp: {
      name: 'PvP',
      type: 'toggle',
      descriptionFunc: (pvp) =>
        pvp
          ? 'Players can directly attack each other.'
          : 'Players cannot directly attack each other.',
    },
    'online-mode': {
      name: 'Online Mode',
      type: 'toggle',
      descriptionFunc: (onlineMode) =>
        onlineMode
          ? 'Players must be authenticated with Xbox Live or Mojang to join.'
          : 'Players can join without authentication and with any username.',
    },
    motd: {
      name: 'MOTD: Message of the Day',
      type: 'text',
      descriptionFunc: (motd) => (
        <div
          className={`mt-1 whitespace-pre-wrap p-2 font-minecraft text-base text-[gray]`}
          style={{ backgroundImage: `url(/assets/dirt.png)` }}
        >
          <MOTDRender motd={parse(convertUnicode(motd))} />
        </div>
      ),
    },
  };

  // hand picked list of minecraft settings to be shown
  const advancedSettings: {
    [key: string]: {
      name: string;
      type: 'toggle' | 'number' | 'text' | 'dropdown';
      options?: string[];
      description?: string;
      descriptionFunc?: (value: any) => string;
    };
  } = {
    'white-list': {
      name: 'Whitelist',
      type: 'toggle',
      descriptionFunc: (value: boolean) =>
        value ? 'Only whitelisted players can join.' : 'All players can join.',
    },
    'enforce-whitelist': {
      name: 'Enforce Whitelist',
      type: 'toggle',
      descriptionFunc: (value: boolean) =>
        value
          ? 'Online players not on the whitelist are kicked.'
          : 'Online players are not kicked even if they are not on the whitelist.',
    },
    'enable-command-block': {
      name: 'Command Blocks',
      type: 'toggle',
      descriptionFunc: (value: boolean) =>
        value ? 'Command blocks are enabled.' : 'Command blocks are disabled.',
    },
    'allow-flight': {
      name: 'Flight',
      type: 'toggle',
      descriptionFunc: (value: boolean) =>
        value
          ? 'Survival players with a fly mod can fly.'
          : 'Survival players in air for 5 seconds will be kicked.',
    },
    'force-gamemode': {
      name: 'Force Gamemode',
      type: 'toggle',
      descriptionFunc: (value: boolean) =>
        value
          ? 'Players join in the default gamemode.'
          : 'Players join in the gamemode they left in.',
    },
    'simulation-distance': {
      name: 'Simulation Distance',
      type: 'number',
      descriptionFunc: (value: number) =>
        `Living entities within ${value} chunks of a player will be simulated.`,
    },
    'player-idle-timeout': {
      name: 'Player Idle Timeout',
      type: 'number',
      descriptionFunc: (value: number) =>
        value == 0
          ? 'Players will not be kicked for inactivity'
          : `Players will be kicked after ${value} minutes of inactivity.`,
    },
    'enforce-secure-profile': {
      name: 'Enforce Secure Profile',
      type: 'toggle',
      descriptionFunc: (value: boolean) =>
        value
          ? 'Players without a Mojang-signed public key will not be able to connect to the server'
          : "Players don't need a Mojang-signed public key to connect to the server.",
    },
  };

  // filter out unsupported settings
  const availableCommonSettings = Object.keys(commonSettings).filter((key) =>
    supportedSettings.includes(key)
  );
  const availableAdvancedSettings = Object.keys(advancedSettings).filter(
    (key) => supportedSettings.includes(key)
  );

  if (isLoading) {
    return <div>Loading...</div>;
    // TODO: show an unobtrusive loading screen, reduce UI flicker
  }

  return (
    <>
      <div className="flex flex-col gap-4 @4xl:flex-row">
        <div className="w-72 shrink-0">
          <h1 className="text-large font-black"> Common Settings </h1>
          <h2 className="text-base font-medium italic tracking-tight text-white/50">
            Common settings that are used by most servers.
          </h2>
        </div>
        <div className="w-full min-w-0 rounded-lg border border-gray-faded/30 child:w-full child:border-b child:border-gray-faded/30 first:child:rounded-t-lg last:child:rounded-b-lg last:child:border-b-0">
          {availableCommonSettings.length ? (
            availableCommonSettings.map((setting) => {
              return (
                <SettingField
                  instance={instance}
                  descriptionFunc={commonSettings[setting].descriptionFunc}
                  setting={setting}
                  label={commonSettings[setting].name}
                  options={commonSettings[setting].options}
                  key={setting}
                  // eslint-disable-next-line @typescript-eslint/no-explicit-any
                  type={commonSettings[setting].type as any}
                />
              );
            })
          ) : (
            <div className="flex h-full w-full flex-col items-center justify-center bg-gray-800 p-4">
              <h1 className="text-large font-bold">
                {' '}
                Not available at this moment{' '}
              </h1>
              <h2 className="text-base font-medium tracking-tight text-gray-300">
                Try to start this instance at least once.
              </h2>
            </div>
          )}
        </div>
      </div>
      <div className="flex flex-col gap-4 @4xl:flex-row">
        <div className="w-72 shrink-0">
          <h1 className="text-large font-black"> Advanced Settings </h1>
          <h2 className="text-base font-medium italic tracking-tight text-white/50">
            Most users should not need to change these settings.
          </h2>
        </div>
        <div className="w-full min-w-0 rounded-lg border border-gray-faded/30 child:w-full child:border-b child:border-gray-faded/30 first:child:rounded-t-lg last:child:rounded-b-lg last:child:border-b-0">
          {availableAdvancedSettings.length ? (
            availableAdvancedSettings.map((setting) => {
              return (
                <SettingField
                  instance={instance}
                  descriptionFunc={advancedSettings[setting].descriptionFunc}
                  setting={setting}
                  label={advancedSettings[setting].name}
                  options={advancedSettings[setting].options}
                  key={setting}
                  // eslint-disable-next-line @typescript-eslint/no-explicit-any
                  type={advancedSettings[setting].type as any}
                />
              );
            })
          ) : (
            <div className="flex h-full w-full flex-col items-center justify-center bg-gray-800 p-4">
              <h1 className="text-large font-bold">
                {' '}
                Not available at this moment{' '}
              </h1>
              <h2 className="text-base font-medium tracking-tight text-gray-300">
                Try to start this instance at least once.
              </h2>
            </div>
          )}
        </div>
      </div>
      <div className="mb-16 flex flex-col gap-4 @4xl:flex-row">
        <div className="w-72 shrink-0">
          <h1 className="text-large font-black"> Danger Zone </h1>
          <h2 className="text-base font-medium italic tracking-tight text-white/50">
            These settings can cause irreversible damage to your server.
          </h2>
        </div>
        <div className="w-full min-w-0 rounded-lg border border-gray-faded/30 child:w-full child:border-b child:border-gray-faded/30 first:child:rounded-t-lg last:child:rounded-b-lg last:child:border-b-0">
          <div className="group relative flex h-full flex-row items-center justify-between gap-4 bg-gray-800 px-4 py-3 text-base">
            <div className="flex min-w-0 grow flex-col">
              {can_delete_instance ? (
                <label className="text-base font-medium text-red-200">
                  Delete Instance
                </label>
              ) : (
                <label className="text-base font-medium text-gray-300">
                  Delete Instance
                </label>
              )}
              {can_delete_instance ? (
                <div className="overflow-hidden text-ellipsis text-small font-medium tracking-medium text-red">
                  Delete this game instance and all of its data.
                </div>
              ) : (
                <div className="overflow-hidden text-ellipsis text-small font-medium tracking-medium text-white/50">
                  No permission
                </div>
              )}
            </div>
            <div className="relative flex w-5/12 shrink-0 flex-row items-center justify-end gap-4">
              <Button
                label="Delete"
                color="danger"
                disabled={!can_delete_instance}
                onClick={() => {
                  axiosWrapper({
                    method: 'DELETE',
                    url: `/instance/${instance.uuid}`,
                  })
                    .then(() => {
                      queryClient.invalidateQueries(['instances', 'list']);
                      selectInstance(undefined);
                    })
                    .catch((err) => {
                      const err_message = errorToString(err);
                      alert(err_message);
                    });
                }}
              />
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

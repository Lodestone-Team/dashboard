import Button from 'components/Atoms/Button';
import { LodestoneContext } from 'data/LodestoneContext';
import { useUid, useUserInfo } from 'data/UserInfo';
import { Fragment, useContext, useEffect, useState } from 'react';
import {
  faCaretDown,
  faArrowRightArrowLeft,
  faBell,
  faCog,
  faRightFromBracket,
  faUser,
} from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Menu, Popover, Transition } from '@headlessui/react';
import { InstanceContext } from 'data/InstanceContext';
import { BrowserLocationContext } from 'data/BrowserLocationContext';
import { CoreInfo, useCoreInfo } from 'data/SystemInfo';
import { AxiosError } from 'axios';
import Label, { LabelColor } from 'components/Atoms/Label';
import Avatar from 'components/Atoms/Avatar';
import NotificationPanel from './NotificationPanel';
import TopBanner from 'components/Atoms/TopBanner';

export type UserState = 'loading' | 'logged-in' | 'logged-out';

export default function TopNav() {
  const { setPathname, setSearchParam } = useContext(BrowserLocationContext);
  const { isLoading, isError, data: user } = useUserInfo();
  const [userState, setUserState] = useState<UserState>('logged-out');
  const uid = useUid();
  const { token, setToken, core, coreConnectionStatus } =
    useContext(LodestoneContext);
  const { address, port } = core;
  const socket = `${address}:${port}`;
  const { data: coreData } = useCoreInfo();

  const statusMap = {
    loading: 'Connecting',
    error: 'Error',
    success: 'Connected',
    degraded: 'Degraded',
  };

  const colorMap: Record<string, LabelColor> = {
    loading: 'gray',
    error: 'red',
    success: 'green',
    degraded: 'yellow',
  };

  useEffect(() => {
    if (!token) {
      setUserState('logged-out');
    } else if (isLoading) {
      setUserState('loading');
      return;
    } else if (isError) {
      setUserState('logged-out');
      return;
    } else {
      setUserState('logged-in');
    }
  }, [token, isLoading, isError, user]);

  return (
    <>
      {coreConnectionStatus === 'degraded' && (
        <TopBanner intention="warning">
          <p>
            <a
              href="#"
              className="font-bold hover:underline"
              onClick={() => {
                window.location.reload();
              }}
            >
              Refresh
            </a>{' '}
            to get the latest data on Firefox.{" "}
            <a
              href="https://github.com/Lodestone-Team/lodestone/wiki/Known-Issues#firefox"
              target="_blank"
              rel="noreferrer"
              className="hover:underline"
            >
              Learn more
            </a>
          </p>
        </TopBanner>
      )}
      <div className="flex w-full shrink-0 flex-row items-center justify-end gap-4 border-b border-gray-faded/30 bg-gray-800 px-4 py-2">
        <div className="grow">
          <img
            src="/logo.svg"
            alt="logo"
            className="w-32 hover:cursor-pointer"
            onClick={() => {
              setSearchParam('instance', undefined);
              setSearchParam('user', undefined);
              setPathname('/');
            }}
          />
        </div>
        <div className="flex flex-row flex-wrap items-baseline gap-1">
          <p className="text-center text-medium font-medium text-white/50">
            {coreData?.core_name ?? '...'}:
          </p>
          <Label
            size="small"
            color={colorMap[coreConnectionStatus]}
            className="w-20 text-center"
          >
            {statusMap[coreConnectionStatus]}
          </Label>
        </div>
        <FontAwesomeIcon
          icon={faCog}
          className="w-4 select-none text-white/50 hover:cursor-pointer hover:text-white/75"
          onClick={() => {
            setPathname('/settings');
          }}
        />
        <Popover className="relative">
          <Popover.Button
            as={FontAwesomeIcon}
            icon={faBell}
            className="w-4 select-none hover:cursor-pointer ui-open:text-gray-300 ui-not-open:text-white/50 ui-not-open:hover:text-white/75"
          />
          <Popover.Panel className="absolute right-0 z-40 mt-1 h-[80vh] w-[480px] rounded-lg drop-shadow-lg child:h-full">
            <NotificationPanel className="rounded-lg border" />
          </Popover.Panel>
        </Popover>
        <Menu as="div" className="relative inline-block text-left">
          <Menu.Button
            as={Button}
            loading={userState === 'loading'}
            label={
              userState === 'logged-in' && user
                ? `Hi, ${user.username}`
                : 'Guest'
            }
            iconComponent={
              userState == 'logged-in' ? (
                <Avatar name={uid} />
              ) : (
                <FontAwesomeIcon icon={faUser} className="w-4 opacity-50" />
              )
            }
            iconRight={faCaretDown}
          ></Menu.Button>
          <Transition
            as={Fragment}
            enter="transition ease-out duration-200"
            enterFrom="opacity-0 -translate-y-1"
            enterTo="opacity-100 translate-y-0"
            leave="transition ease-in duration-150"
            leaveFrom="opacity-100 translate-y-0"
            leaveTo="opacity-0 -translate-y-1"
          >
            <Menu.Items className="absolute right-0 z-10 mt-1.5 origin-top-left divide-y divide-gray-faded/30 rounded border border-gray-faded/30 bg-gray-800 drop-shadow-md focus:outline-none">
              <div className="py-2 px-1.5">
                <Menu.Item>
                  {({ disabled }) => (
                    <Button
                      className="w-full flex-nowrap whitespace-nowrap"
                      label={userState === 'logged-in' ? 'Sign out' : 'Sign in'}
                      loading={userState === 'loading'}
                      iconRight={faRightFromBracket}
                      onClick={() => {
                        // remove the current token
                        // a logged out user will be auto-redirected to the login page
                        setToken('', socket);
                        setSearchParam('instance', undefined);
                        setSearchParam('user', undefined);
                      }}
                      align="end"
                      disabled={disabled}
                      variant="text"
                    />
                  )}
                </Menu.Item>

                <Menu.Item>
                  {({ disabled }) => (
                    <Button
                      className="w-full flex-nowrap whitespace-nowrap"
                      label="Change core"
                      iconRight={faArrowRightArrowLeft}
                      align="end"
                      disabled={disabled}
                      onClick={() => {
                        setSearchParam('instance', undefined);
                        setSearchParam('user', undefined);
                        setPathname('/login/core/select');
                      }}
                      variant="text"
                    />
                  )}
                </Menu.Item>
              </div>
            </Menu.Items>
          </Transition>
        </Menu>
      </div>
    </>
  );
}

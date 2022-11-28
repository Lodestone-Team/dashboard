// A react component that renders the left and top navbar for the dashboard.
// Also provides the instance context

import NoSSR from 'react-no-ssr';
import LeftNav from './LeftNav';
import TopNav from './TopNav';
import {
  useInterval,
  useLocalStorage,
  useSessionStorage,
  useWindowSize,
} from 'usehooks-ts';
import { useEventStream } from 'data/EventStream';
import { useClientInfo } from 'data/SystemInfo';
import { InstanceContext } from 'data/InstanceContext';
import { InstanceInfo } from 'bindings/InstanceInfo';
import { useEffect, useState } from 'react';
import { useInstanceList } from 'data/InstanceList';
import { useRouterQuery } from 'utils/hooks';
import router from 'next/router';
import ResizePanel from 'components/Atoms/ResizePanel';
import NotificationPanel from './NotificationPanel';

export default function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const { query: uuid } = useRouterQuery('uuid');
  const { isLoading, isError, data: instances, error } = useInstanceList();
  const [instance, setInstanceState] = useState<InstanceInfo | null>(null);
  const [rightNavSize, setRightNavSize] = useLocalStorage('rightNavSize', 200);
  const [showNotifications, setShowNotifications] = useLocalStorage(
    'showNotifications',
    false
  );
  const { width, height } = useWindowSize();

  // called for side effects
  useEventStream();
  useClientInfo();

  useEffect(() => {
    if (uuid && instances && uuid in instances) setInstanceState(instances[uuid]);
    else setInstanceState(null);
  }, [instances, uuid]);

  function setInstance(instance: InstanceInfo | null) {
    if (instance === null) {
      setInstanceState(null);
      router.push(
        {
          pathname: '/',
          query: {
            ...router.query,
            uuid: null,
          },
        },
        undefined,
        { shallow: true }
      );
    } else {
      setInstanceState(instance);
      router.push(
        {
          pathname: '/dashboard',
          query: {
            ...router.query,
            uuid: instance.uuid,
          },
        },
        undefined,
        { shallow: true }
      );
    }
  }

  return (
    <InstanceContext.Provider
      value={{
        instanceList: instances || {},
        selectedInstance: instance,
        selectInstance: setInstance,
      }}
    >
      <div className="flex h-screen flex-col">
        <NoSSR>
          <TopNav
            showNotifications={showNotifications}
            setShowNotifications={setShowNotifications}
          />
        </NoSSR>
        <div className="relative flex min-h-0 w-full grow flex-row bg-gray-850">
          <div className="flex grow flex-row justify-center gap-[1vw]">
            <div className="flex h-full grow basis-60 flex-row flex-nowrap items-stretch justify-end">
              <div className="h-full w-[16rem] max-w-[16rem] child:h-full">
                <LeftNav />
              </div>
            </div>
            <div className="h-full min-w-0 grow basis-[1024px] child:h-full">
              <div className="max-w-[1024px]">{children}</div>
            </div>
          </div>
          <NoSSR>
            {showNotifications &&
              (width > 1280 ? (
                <ResizePanel
                  direction="w"
                  maxSize={500}
                  minSize={200}
                  size={rightNavSize}
                  validateSize={false}
                  onResize={setRightNavSize}
                  containerClassNames="min-h-0"
                >
                  <NotificationPanel />
                </ResizePanel>
              ) : (
                <div
                  className="absolute right-2 -top-2 h-full w-96 rounded-lg drop-shadow-lg child:h-full"
                  style={
                    {
                      width: rightNavSize,
                    }
                  }
                >
                  <NotificationPanel className="rounded-lg border" />
                </div>
              ))}
          </NoSSR>
        </div>
      </div>
    </InstanceContext.Provider>
  );
}

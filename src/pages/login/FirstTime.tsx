import Button from 'components/Atoms/Button';
import { useContext } from 'react';
import { DEFAULT_LOCAL_CORE } from 'utils/util';
import { BrowserLocationContext } from 'data/BrowserLocationContext';
import { useQueryClient } from '@tanstack/react-query';
import { useGlobalSettings } from 'data/GlobalSettings';
import { LodestoneContext } from 'data/LodestoneContext';
import { useDocumentTitle, useEffectOnce } from 'usehooks-ts';
import { tauri } from 'utils/tauriUtil';

const FirstTime = () => {
  useDocumentTitle('Welcome to Lodestone');
  const { setPathname } = useContext(BrowserLocationContext);
  const { coreList, addCore, setCore } = useContext(LodestoneContext);

  useEffectOnce(() => {
    if (coreList.length > 0) {
      setPathname('/login/core/select');
      return;
    }
    if (!tauri) return;
    tauri
      .invoke<string | null>('is_setup')
      .then((is_setup) => {
        addCore(DEFAULT_LOCAL_CORE);
        setCore(DEFAULT_LOCAL_CORE);
        if (is_setup) {
          setPathname('/');
        } else {
          setPathname('/login/core/first_setup');
        }
      })
      .catch((err: any) => {
        console.log('Tauri call failed is_setup', err);
      });
  });

  return (
    <div className="flex w-[640px] max-w-full flex-col items-stretch justify-center gap-16 rounded-2xl bg-gray-850 px-12 py-16 transition-dimensions @container">
      <div className="text flex flex-col items-start">
        <img src="/logo.svg" alt="logo" className="h-fit w-fit" />
        <h1 className="font-title text-h1 font-bold tracking-medium text-gray-300">
          Welcome to Lodestone
        </h1>
        <h2 className="text-medium font-medium tracking-medium text-white/75">
          Learn more about Lodestone and any known issues on our {" "}
          <a
            href="https://github.com/Lodestone-Team/lodestone/wiki"
            target="_blank"
            rel="noreferrer"
            className="text-blue-200 underline hover:text-blue-300"
          >
            here.
          </a>
        </p>

        <p className="text-medium font-medium tracking-medium text-white">
          Our product is still in its beta release cycle. Browser support is
          limited and bugs are expected. You can check known issues and report
          any new ones on our {' '}
          <a
            href="https://github.com/Lodestone-Team/lodestone"
            target="_blank"
            rel="noreferrer"
            className="text-blue-200 underline hover:text-blue-300"
          >
            Github.
          </a>
        </p>
      </div>

      <div className="flex flex-row items-baseline gap-4">
        <Button
          className="flex-1"
          label="Download Lodestone Core"
          onClick={() => {
            window.open(
              'https://github.com/Lodestone-Team/dashboard/releases/',
              '_self'
            );
          }}
          intention="primary"
          size="large"
        />

        <Button
          className="flex-1"
          label="Connect to existing Core"
          onClick={() => setPathname('/login/core/new')}
          intention="primary"
          size="large"
        />
      </div>
    </div>
  );
};

export default FirstTime;

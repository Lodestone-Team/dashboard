import { InstanceSettingCard } from 'components/Instance';
import GameConsole from 'components/GameConsole';
import FileViewer from 'components/FileViewer';
import DashboardCard from 'components/DashboardCard';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import InstanceOverview from 'components/Instance/InstanceOverview';
import { match, otherwise } from 'variant';

const unknown_icon = '/assets/minecraft-missing-texture.svg';

import {
  faChartLine,
  faCodeCompare,
  faCog,
  faFolder,
  faInbox,
  faServer,
} from '@fortawesome/free-solid-svg-icons';
import { HandlerGameType } from 'bindings/HandlerGameType';
import { Game } from 'bindings/Game';

type InstanceTab = {
  title: string;
  displayTitle: string | null;
  path: string;
  width: string;
  icon: JSX.Element;
  content: JSX.Element;
};

export const game_to_game_icon = (game: Game) =>
  match(game, {
    MinecraftJava: ({ variant }) =>
      match(
        variant,
        otherwise(
          {
            Vanilla: () => '/assets/minecraft-vanilla.png',
            Fabric: () => '/assets/minecraft-fabric.png',
            Forge: () => '/assets/minecraft-forge.png',
            Paper: () => '/assets/minecraft-paper.png',
              Neoforge: () => '/assets/minecraft-neoforge.png',
          },
          () => unknown_icon
        )
      ),
    Generic: () => '/assets/GenericIcon.svg',
  });

export const game_to_game_title = (game: Game) =>
  match(game, {
    MinecraftJava: ({ variant }) =>
      match(variant, {
        Vanilla: () => 'Minecraft',
        Forge: () => 'Forge (Minecraft)',
        Fabric: () => 'Fabric (Minecraft)',
        Paper: () => 'Paper (Minecraft)',
        Spigot: () => 'Spigot (Minecraft)',
        Neoforge: () => 'Neoforge (Minecraft)',
        Other: ({ name }) => `${name} (Minecraft)`,
      }),
    Generic: ({ game_name }) => `${game_name} (Generic)`,
  });

export const game_to_description = (game: Game) =>
  match(game, {
    MinecraftJava: ({ variant }) =>
      match(variant, {
        Vanilla: () => 'Standard vanilla Minecraft server from Mojang.',
        Forge: () =>
          'Modding framework that allows you to install mods and customize your Minecraft experience.',
        Fabric: () => 'Lightweight modding toolchain for Minecraft.',
        Paper: () =>
          'High-performance Spigot fork that aims to fix gameplay and mechanics inconsistencies.',
        Spigot: () =>
          'Modified Minecraft server software that supports plugins, offering enhanced performance and customization options.',
        Neoforge: () =>
            'Forge fork focused on enhancing modding capabilities and maintaining compatibility.',
        Other: ({ name }) => `Unknown Minecraft variant: ${name}`,
      }),
    Generic: ({ game_name }) => `Unknown game: ${game_name}`,
  });

export const HandlerGameType_to_Game: Record<HandlerGameType, Game> = {
  MinecraftJavaVanilla: {
    type: 'MinecraftJava',
    variant: {
      type: 'Vanilla',
    },
  },
  MinecraftFabric: {
    type: 'MinecraftJava',
    variant: {
      type: 'Fabric',
    },
  },
  MinecraftForge: {
    type: 'MinecraftJava',
    variant: {
      type: 'Forge',
    },
  },
  MinecraftPaper: {
    type: 'MinecraftJava',
    variant: {
      type: 'Paper',
    },
  },
    MinecraftNeoforge: {
      type: 'MinecraftJava',
        variant: {
          type: 'Neoforge'
        }
    }
};

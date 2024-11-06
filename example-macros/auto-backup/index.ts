import { format } from "https://deno.land/std@0.177.1/datetime/format.ts";
import { copy } from "https://deno.land/std@0.191.0/fs/copy.ts";
import { sleep } from "https://deno.land/x/sleep@v1.2.1/mod.ts";
import { EventStream } from "https://raw.githubusercontent.com/Lodestone-Team/lodestone-macro-lib/main/events.ts";
import { lodestoneVersion } from "https://raw.githubusercontent.com/Lodestone-Team/lodestone-macro-lib/main/prelude.ts";
import { MinecraftJavaInstance } from "https://raw.githubusercontent.com/Lodestone-Team/lodestone-macro-lib/main/instance.ts";
import { compress } from "https://deno.land/x/zip@v1.2.5/mod.ts";

const currentInstance = await MinecraftJavaInstance.current();

const eventStream = new EventStream(
  currentInstance.getUUID(),
  await currentInstance.name()
);

// Lodestone will parse the configuration class and inject the configuration into the macro
class LodestoneConfig {
  // Where to store the backups relative to the instance path
  backupFolderRelative: string = "backups";
  // How long to wait between backups in seconds
  delaySec: number = 3600;
  // Comma-separated list of folders to back up
  foldersToBackup: string = "world, world_nether, world_the_end";
  // Compress the backup? 
  compressBackup: boolean = true;
}

// not technically necessary, but it's a good practice to appease the linter
declare const config: LodestoneConfig;

// make sure the config is injected properly
console.log(config);

const instancePath = await currentInstance.path();
const backupFolder = `${instancePath}/${config.backupFolderRelative}`;
EventStream.emitDetach();
while (true) {
  eventStream.emitConsoleOut("[Backup Macro] Backing up world...");
  if ((await currentInstance.state()) == "Stopped") {
    eventStream.emitConsoleOut("[Backup Macro] Instance stopped, exiting...");
    break;
  }

  const now = new Date();
  const now_str = format(now, "yy-MM-dd_HH");
  const combinedBackupFolder = `${backupFolder}/backup_${now_str}`;
  // Split the string by commas to get an array of folders and iterate over them
  for (const folder of config.foldersToBackup.split(',')) {
    const trimmedFolder = folder.trim();
    try {
      const sourceFolder = `${instancePath}/${trimmedFolder}`;
      const destinationFolder = `${combinedBackupFolder}/${trimmedFolder}`;
      await eventStream.emitConsoleOut(`[Backup Macro] Backing up ${trimmedFolder}...`);
      await copy(sourceFolder, destinationFolder);
      await eventStream.emitConsoleOut(`[Backup Macro] Backup of ${trimmedFolder} completed.`);
    } catch (e) {
      console.log(`[Backup Macro] Error backing up ${trimmedFolder}:`, e);
    }
  }

  if (config.compressBackup) {
    try {
      await eventStream.emitConsoleOut(`[Backup Macro] Compressing backup folder: ${combinedBackupFolder}...`);
      await compress(`${combinedBackupFolder}/`, `${combinedBackupFolder}.zip`);
      await eventStream.emitConsoleOut(`[Backup Macro] Compression completed.`);
    } catch (e) {
      console.log(`[Backup Macro] Error compressing backup folder:`, e);
    }
  }

  await sleep(config.delaySec);
}


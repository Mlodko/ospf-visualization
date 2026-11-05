from os import makedirs
import shutil
import sys

CONFIG_DIR = "startup_configs"
CONFIG_SUBDIRECTORIES = {
    "rca1": "as_1/area_0",
    "rca2": "as_1/area_0",
    "rca3": "as_1/area_0",
    "rca4": "as_1/area_0",
    "rca5": "as_1/area_0",
    "rda11": "as_1/area_1",
    "rda12": "as_1/area_1",
    "rda13": "as_1/area_1",
    "rda14": "as_1/area_1",
    "rda31": "as_1/area_3",
    "rda32": "as_1/area_3",
    "rcb1": "as_2/area_0",
    "rcb2": "as_2/area_0",
    "rcb3": "as_2/area_0",
    "rdb11": "as_2/area_1",
    "rdb12": "as_2/area_1"
}

RUNNING_CONFIG_DIR = "clab-cEOS-Lab"

# Path to running config of router {RXX}
# {RUNNING_CONFIG_DIR}/{RXX}/flash/startup-config

def get_run_config_paths():
    for router in CONFIG_SUBDIRECTORIES.keys():
        yield f"{RUNNING_CONFIG_DIR}/{router}/flash/running-config"

def construct_save_path(router: str, subdir: str):
    return f"{CONFIG_DIR}/{subdir}/{router}.conf"

def main():
    dry_run = False
    if len(sys.argv) > 1:
        if "--dry-run" in sys.argv[1:]:
            dry_run = True

    for router, subdir in CONFIG_SUBDIRECTORIES.items():
        run_config_path = f"{RUNNING_CONFIG_DIR}/{router}/flash/startup-config"
        save_path = construct_save_path(router, subdir)
        print(f"Saving running config of {router} from {run_config_path} to {save_path}")
        if not dry_run:
            makedirs(f"{CONFIG_DIR}/{subdir}", exist_ok=True)
            _ = shutil.copy(run_config_path, save_path)

if __name__ == "__main__":
    main()    
from __future__ import annotations

from pathlib import Path
from typing import Any

# This program generates SR Linux CLI startup configs from a simple Python dict.
# The result is intended to be pasteable into an SR Linux candidate config or saved
# as a startup-config file for a ContainerLab node.
#
# Assumptions:
# - each router has one dataplane default network-instance
# - each logical interface gets a .0 subinterface for L3 addresses
# - OSPF is attached explicitly per-area/per-interface

configs = {
    "rca1": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.0.12.1/24"},
            "ethernet-1/2": {"ip_address": "10.0.14.1/24"},
            "ethernet-1/3": {"ip_address": "10.0.13.1/24"},
            "ethernet-1/4": {"ip_address": "10.102.1.1/24"},
            "ethernet-1/5": {"ip_address": "120.0.1.0/31"},
            "lo0": {"ip_address": "100.1.0.1/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.0.1",
                "areas": {
                    "0.0.0.0": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "ethernet-1/3.0",
                            "lo0.0",
                        ]
                    }
                },
            }
        },
    },
    "rca2": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.0.12.2/24"},
            "ethernet-1/2": {"ip_address": "10.0.23.2/24"},
            "ethernet-1/3": {"ip_address": "10.0.24.2/24"},
            "ethernet-1/4": {"ip_address": "10.101.2.2/24"},
            "ethernet-1/5": {"ip_address": "120.0.2.0/31"},
            "lo0": {"ip_address": "100.1.0.2/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.0.2",
                "areas": {
                    "0.0.0.0": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "ethernet-1/3.0",
                            "lo0.0",
                        ]
                    },
                    "0.0.0.1": {
                        "interfaces": [
                            "ethernet-1/4.0",
                        ]
                    },
                },
            }
        },
    },
    "rca3": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.0.23.3/24"},
            "ethernet-1/2": {"ip_address": "10.0.34.3/24"},
            "ethernet-1/3": {"ip_address": "10.0.13.3/24"},
            "ethernet-1/4": {"ip_address": "10.101.3.3/24"},
            "ethernet-1/5": {"ip_address": "10.0.35.3/24"},
            "lo0": {"ip_address": "100.1.0.3/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.0.3",
                "areas": {
                    "0.0.0.0": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "ethernet-1/3.0",
                            "ethernet-1/5.0",
                            "lo0.0",
                        ]
                    },
                    "0.0.0.1": {
                        "interfaces": [
                            "ethernet-1/4.0",
                        ]
                    },
                },
            }
        },
    },
    "rca4": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.0.34.4/24"},
            "ethernet-1/2": {"ip_address": "10.0.14.4/24"},
            "ethernet-1/3": {"ip_address": "10.0.24.4/24"},
            "ethernet-1/5": {"ip_address": "10.0.45.4/24"},
            "lo0": {"ip_address": "100.1.0.4/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.0.4",
                "areas": {
                    "0.0.0.0": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "ethernet-1/3.0",
                            "ethernet-1/5.0",
                            "lo0.0",
                        ]
                    }
                },
            }
        },
    },
    "rca5": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.102.5.5/24"},
            "ethernet-1/2": {"ip_address": "10.101.5.5/24"},
            "ethernet-1/3": {"ip_address": "10.103.5.5/24"},
            "ethernet-1/4": {"ip_address": "10.0.35.5/24"},
            "ethernet-1/5": {"ip_address": "10.0.45.5/24"},
            "lo0": {"ip_address": "100.1.0.5/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.0.5",
                "areas": {
                    "0.0.0.1": {
                        "interfaces": [
                            "ethernet-1/2.0",
                        ]
                    },
                    "0.0.0.2": {
                        "interfaces": [
                            "ethernet-1/1.0",
                        ]
                    },
                    "0.0.0.3": {
                        "interfaces": [
                            "ethernet-1/3.0",
                        ],
                        "stub": True,
                    },
                    "0.0.0.0": {
                        "interfaces": [
                            "lo0.0",
                            "ethernet-1/4.0",
                            "ethernet-1/5.0",
                        ]
                    },
                },
            }
        },
    },
    "rda11": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.101.2.11/24"},
            "ethernet-1/2": {"ip_address": "10.101.3.11/24"},
            "ethernet-1/3": {"ip_address": "10.1.123.1/24"},
            "lo0": {"ip_address": "100.1.1.1/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.1.1",
                "areas": {
                    "0.0.0.1": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "ethernet-1/3.0",
                            "lo0.0",
                        ]
                    }
                }
            }
        }
    },
    "rda12": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.1.123.2/24"},
            "lo0": {"ip_address": "100.1.1.2/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.1.2",
                "areas": {
                    "0.0.0.1": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "lo0.0",
                        ]
                    },
                },
            },
        },
    },
    "rda13": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.1.123.3/24"},
            "ethernet-1/2": {"ip_address": "10.1.34.3/24"},
            "lo0": {"ip_address": "100.1.1.3/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.1.3",
                "areas": {
                    "0.0.0.1": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "lo0.0",
                        ]
                    }
                },
            },
        },
    },
    "rda14": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.1.34.4/24"},
            "ethernet-1/2": {"ip_address": "10.101.5.14/24"},
            "lo0": {"ip_address": "100.1.1.4/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.1.4",
                "areas": {
                    "0.0.0.1": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "lo0.0",
                        ]
                    },
                },
            },
        },
    },
    "rda21": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.102.1.21/24"},
            "ethernet-1/2": {"ip_address": "10.102.4.21/24"},
            "ethernet-1/3": {"ip_address": "10.2.13.1/24"},
            "ethernet-1/4": {"ip_address": "10.2.12.1/24"},
            "lo0": {"ip_address": "100.1.2.1/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.2.1",
                "areas": {
                    "0.0.0.2": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "ethernet-1/3.0",
                            "ethernet-1/4.0",
                            "lo0.0",
                        ]
                    }
                },
            },
        },
    },
    "rda22": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.2.12.2/24"},
            "ethernet-1/2": {"ip_address": "10.2.24.2/24"},
            "lo0": {"ip_address": "100.1.2.2/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.2.2",
                "areas": {
                    "0.0.0.2": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "lo0.0",
                        ]
                    }
                },
            },
        }
    },
    "rda23": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.2.13.3/24"},
            "ethernet-1/2": {"ip_address": "10.2.34.3/24"},
            "lo0": {"ip_address": "100.1.2.3/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.2.3",
                "areas": {
                    "0.0.0.2": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "lo0.0",
                        ]
                    },
                },
            },
        },
    },
    "rda24": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.2.34.3/24"},
            "ethernet-1/2": {"ip_address": "10.2.24.4/24"},
            "ethernet-1/3": {"ip_address": "10.102.5.24/24"},
            "lo0": {"ip_address": "100.1.2.4/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.2.4",
                "areas": {
                    "0.0.0.2": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "ethernet-1/3.0",
                            "lo0.0",
                        ]
                    },
                },
            },
        },
    },
    "rda31": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.103.5.31/24"},
            "ethernet-1/2": {"ip_address": "10.3.12.1/24"},
            "lo0": {"ip_address": "100.1.3.1/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.3.1",
                "areas": {
                    "0.0.0.3": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "ethernet-1/2.0",
                            "lo0.0",
                        ],
                        "stub": True,
                    },
                },
            },
        },
    },
    "rda32": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "10.3.12.2/24"},
            "lo0": {"ip_address": "100.1.3.2/32"},
        },
        "protocols": {
            "ospf": {
                "router_id": "100.1.3.2",
                "areas": {
                    "0.0.0.3": {
                        "interfaces": [
                            "ethernet-1/1.0",
                            "lo0.0",
                        ],
                        "stub": True,
                    },
                },
            },
        },
    },

    "rcb1": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "120.0.1.1/31"},
            "ethernet-1/2": {"ip_address": "20.0.12.1/24"},
            "ethernet-1/3": {"ip_address": "20.0.13.1/24"},
            "lo0": {"ip_address": "100.2.0.1/32"},
        },
        "protocols": {
            "isis": {
                "level": "2", # 1, 2, or 1-2
                "net": "49.0000.0000.0000.0001.00",
                "interfaces": {
                    "ethernet-1/2.0": {
                        "level": "2"
                    },
                    "ethernet-1/3.0": {
                        "level": "2"
                    },
                    "lo0.0": {
                        "level": "2"
                    },
                }
            }
        }
    },
    "rcb2": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "120.0.2.1/31"},
            "ethernet-1/2": {"ip_address": "20.0.12.2/24"},
            "ethernet-1/3": {"ip_address": "20.0.23.2/24"},
            "lo0": {"ip_address": "100.2.0.2/32"},
        },
        "protocols": {
            "isis": {
                "level": "2", # 1, 2, or 1-2
                "net": "49.0000.0000.0000.0002.00",
                "interfaces": {
                    "ethernet-1/2.0" : {
                        "level": "2"
                    },
                    "ethernet-1/3.0": {
                        "level": "2"
                    },
                    "lo0.0": {
                        "level": "2"
                    },
                }
            }
        }
    },
    "rcb3": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "20.0.13.3/24"},
            "ethernet-1/2": {"ip_address": "20.0.23.3/24"},
            "ethernet-1/3": {"ip_address": "20.101.12.3/24"},
            "lo0": {"ip_address": "100.2.0.3/32"},
        },
        "protocols": {
            "isis": {
                "level": "1-2", # 1, 2, or 1-2
                "net": "49.0000.0000.0000.0003.00",
                "interfaces": {
                    "ethernet-1/1.0": {
                        "level": "2"
                    },
                    "ethernet-1/2.0": {
                        "level": "2"
                    },
                    "ethernet-1/3.0": {
                        "level": "1"
                    },
                    "lo0.0": {
                        "level": "2"
                    },
                }
            }
        }
    },

    "rdb11": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "20.101.12.11/24"},
            "lo0": {"ip_address": "100.2.1.1/32"},
        },
        "protocols": {
            "isis": {
                "level": "1-2",
                "net": "49.0001.0000.0001.0001.00",
                "interfaces": {
                    "ethernet-1/1.0": {
                        "level": "1"
                    },
                    "lo0.0": {
                        "level": "1"
                    },
                }
            },
        }
    },
    "rdb12": {
        "interfaces": {
            "ethernet-1/1": {"ip_address": "20.101.12.12/24"},
            "lo0": {"ip_address": "100.2.1.2/32"},
        },
        "protocols": {
            "isis": {
                "level": "1-2",
                "net": "49.0001.0000.0001.0002.00",
                "interfaces": {
                    "ethernet-1/1.0": {
                        "level": "1"
                    },
                    "lo0.0": {
                        "level": "1"
                    },
                }
            },
        }
    }
}


def parse_router_config(router_name: str, router_config: dict[str, Any]) -> list[str]:
    commands: list[str] = []
    commands.extend(create_default_network_instance())

    for section, section_config in router_config.items():
        match section:
            case "interfaces":
                for interface_name, interface_config in section_config.items():
                    configure_interface(commands, interface_name, interface_config)
            case "protocols":
                for protocol_name, protocol_config in section_config.items():
                    match protocol_name:
                        case "ospf":
                            configure_ospf(commands, protocol_config)
                        case "isis":
                            configure_isis(commands, protocol_config)
                        case _:
                            raise ValueError(f"Unsupported protocol: {protocol_name}")
            case _:
                raise ValueError(f"Unsupported section: {section}")

    commands.append("commit now")
    commands.append("save startup")
    return commands


def create_default_network_instance() -> list[str]:
    return [
        "enter candidate",
        "set / network-instance default type default",
        "set / network-instance default admin-state enable",
    ]


def configure_interface(commands: list[str], interface_name: str, interface_config: dict[str, Any]) -> None:
    if "ip_address" not in interface_config:
        return

    if interface_name.startswith("lo"):
        commands.append(f"set / interface {interface_name} admin-state enable")
        commands.append(f"set / interface {interface_name} subinterface 0 admin-state enable")
        commands.append(f"set / interface {interface_name} subinterface 0 ipv4 admin-state enable")
        commands.append(f"set / interface {interface_name} subinterface 0 ipv4 address {interface_config['ip_address']}")
        commands.append(f"set / network-instance default interface {interface_name}.0")
        return

    commands.append(f"set / interface {interface_name} admin-state enable")
    commands.append(f"set / interface {interface_name} subinterface 0 admin-state enable")
    commands.append(f"set / interface {interface_name} subinterface 0 ipv4 admin-state enable")
    commands.append(f"set / interface {interface_name} subinterface 0 ipv4 address {interface_config['ip_address']}")
    commands.append(f"set / network-instance default interface {interface_name}.0")


def configure_ospf(commands: list[str], ospf_config: dict[str, Any]) -> None:
    router_id = ospf_config["router_id"]
    commands.append("set / network-instance default protocols ospf instance 1 admin-state enable")
    commands.append(f"set / network-instance default protocols ospf instance 1 router-id {router_id}")
    commands.append("set / network-instance default protocols ospf instance 1 version ospf-v2")

    for area_id, area_config in ospf_config["areas"].items():
        if area_config.get("stub"):
            commands.append(f"set / network-instance default protocols ospf instance 1 area {area_id} stub")

        for interface_name in area_config["interfaces"]:
            commands.append(
                f"set / network-instance default protocols ospf instance 1 area {area_id} interface {interface_name}"
            )

def configure_isis(commands: list[str], isis_config: dict[str, Any]) -> None:
    commands.append("set / network-instance default protocols isis instance 1 admin-state enable")
    commands.append("set / network-instance default protocols isis instance 1 ipv4-unicast admin-state enable")
    
    match level := isis_config.get("level"):
        case "1" | "2":
            commands.append(f"set / network-instance default protocols isis instance 1 level-capability L{level}")
        case "1-2":
            commands.append(f"set / network-instance default protocols isis instance 1 level-capability L1L2")
        case None:
            raise ValueError("Missing 'level' in ISIS configuration")
        case _:
            raise ValueError(f"Unsupported ISIS level: {isis_config['level']}")

    commands.append(f"set / network-instance default protocols isis instance 1 net [{isis_config['net']}]")

    for interface_name, interface_config in isis_config["interfaces"].items():
        if_level = interface_config["level"]
        commands.append(f"set / network-instance default protocols isis instance 1 interface {interface_name} admin-state enable")
        commands.append(f"set / network-instance default protocols isis instance 1 interface {interface_name} level {if_level}")
        

BASE_DIR = Path(__file__).resolve().parent
CONFIG_DIR = BASE_DIR / "startup_configs"
CONFIG_PATHS = {
    "rca1": "as_1/area_0",
    "rca2": "as_1/area_0",
    "rca3": "as_1/area_0",
    "rca4": "as_1/area_0",
    "rca5": "as_1/area_0",

    "rda11": "as_1/area_1",
    "rda12": "as_1/area_1",
    "rda13": "as_1/area_1",
    "rda14": "as_1/area_1",

    "rda21": "as_1/area_2",
    "rda22": "as_1/area_2",
    "rda23": "as_1/area_2",
    "rda24": "as_1/area_2",

    "rda31": "as_1/area_3",
    "rda32": "as_1/area_3",

    "rcb1": "as_2/area_0",
    "rcb2": "as_2/area_0",
    "rcb3": "as_2/area_0",

    "rdb11": "as_2/area_1",
    "rdb12": "as_2/area_1",
}


def main() -> None:
    for router_name, router_config in configs.items():
        cli_commands = parse_router_config(router_name, router_config)
        file_path = CONFIG_DIR / CONFIG_PATHS[router_name] / f"{router_name}.cli"
        save_cli_config(cli_commands, file_path)


def save_cli_config(cli_commands: list[str], file_path: Path) -> None:
    file_path.parent.mkdir(parents=True, exist_ok=True)
    rendered = "\n".join(cli_commands) + "\n"
    file_path.write_text(rendered, encoding="utf-8")
    print(f"Wrote {file_path}")


if __name__ == "__main__":
    main()

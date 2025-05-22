#!/usr/bin/env python3

import time
import socket
import argparse
import os
from scapy.layers.dhcp6 import (
    DHCP6_Solicit, DHCP6_Advertise, DHCP6_Request, DHCP6_Reply,
    DHCP6_Renew, DHCP6_Rebind, DHCP6_Release,
    DHCP6OptClientId, DHCP6OptServerId, DHCP6OptIA_NA, DHCP6OptIAAddress,
    DHCP6OptStatusCode, DHCP6OptElapsedTime, DHCP6OptPref,
    DHCP6OptIA_PD, DHCP6OptIAPrefix
)

SERVER_LISTEN_IP = "::1"
SERVER_PORT = 547

SERVER_DUID = b"\x00\x02\x00\x01\x02\x03\x04\x05\x06\x07"

OFFERED_IPV6_ADDRESS = "2001:db8:cafe::100"
PREFERRED_LIFETIME = 1800
VALID_LIFETIME = 3600

OFFERED_IPV6_PREFIX = "2001:db8:aaaa::"
OFFERED_PREFIX_LENGTH = 64
PREFIX_PREFERRED_LIFETIME = 7200
PREFIX_VALID_LIFETIME = 10800

T1_RATIO = 0.5
T2_RATIO = 0.8
PREFERENCE_VALUE = 255

client_lease_info = {
    "client_duid": None,
    "transaction_id": None,
    "client_link_local": None,
    "client_source_port": None,

    "client_iana_iaid": None,
    "assigned_ip": None,
    "preferred_until": 0,
    "valid_until": 0,
    "na_state": "INIT",

    "client_iapd_iaid": None,
    "assigned_prefix": None,
    "assigned_prefix_length": None,
    "prefix_preferred_until": 0,
    "prefix_valid_until": 0,
    "pd_state": "INIT",
}

SERVER_ID_OPT = DHCP6OptServerId(duid=SERVER_DUID)

def print_state(message):
    timestamp = time.strftime('%Y-%m-%d %H:%M:%S')
    print(f"[{timestamp}] [SERVER] {message}")
    print(f"    Client DUID: {client_lease_info['client_duid']}")
    print(f"    NA State: {client_lease_info['na_state']}")
    if client_lease_info["assigned_ip"]:
        print(f"    Assigned IP: {client_lease_info['assigned_ip']}")
        if client_lease_info["preferred_until"] > 0:
            print(f"    IP Preferred until: {time.ctime(client_lease_info['preferred_until'])}")
            print(f"    IP Valid until: {time.ctime(client_lease_info['valid_until'])}")
    print(f"    PD State: {client_lease_info['pd_state']}")
    if client_lease_info["assigned_prefix"]:
        print(f"    Delegated Prefix: {client_lease_info['assigned_prefix']}/{client_lease_info['assigned_prefix_length']}")
        if client_lease_info["prefix_preferred_until"] > 0:
            print(f"    Prefix Preferred until: {time.ctime(client_lease_info['prefix_preferred_until'])}")
            print(f"    Prefix Valid until: {time.ctime(client_lease_info['prefix_valid_until'])}")

def build_ia_na_option(iaid, ip_address, pref_lft, valid_lft, status_code=0, status_message="Success"):
    iaaddr_opts = []
    if ip_address:
        iaaddr_opts.append(DHCP6OptIAAddress(addr=ip_address, preflft=pref_lft, validlft=valid_lft))
    iaaddr_opts.append(DHCP6OptStatusCode(statuscode=status_code, statusmsg=str(status_message or "Success")))
    return DHCP6OptIA_NA(iaid=iaid, T1=int(pref_lft * T1_RATIO), T2=int(pref_lft * T2_RATIO), ianaopts=iaaddr_opts)

def build_ia_pd_option(iapd_iaid, prefix, plen, pref_lft, valid_lft, status_code=0, status_message="Success"):
    iapdopt_options = []
    if prefix and plen is not None:
        iapdopt_options.append(DHCP6OptIAPrefix(prefix=prefix, plen=plen, preflft=pref_lft, validlft=valid_lft))
    iapdopt_options.append(DHCP6OptStatusCode(statuscode=status_code, statusmsg=str(status_message or "Success")))
    return DHCP6OptIA_PD(iaid=iapd_iaid, T1=int(pref_lft * T1_RATIO), T2=int(pref_lft * T2_RATIO), iapdopt=iapdopt_options)

def send_dhcpv6_response(dest_ip, dest_port, dhcp6_payload):
    if not dest_ip or not dest_port:
        print_state(f"Error: Missing destination IP ({dest_ip}) or port ({dest_port}) for response.")
        return

    sock = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
    try:
        sock.sendto(bytes(dhcp6_payload), (dest_ip, dest_port))
        print_state(f"Sent {dhcp6_payload.__class__.__name__} (TrID: {dhcp6_payload.trid:#x}) to [{dest_ip}]:{dest_port}")
    except Exception as e:
        print_state(f"Error sending packet: {e}")
    finally:
        sock.close()

def reset_client_lease():
    print_state("Resetting client lease information.")
    client_lease_info["client_duid"] = None
    client_lease_info["transaction_id"] = None

    client_lease_info["client_iana_iaid"] = None
    client_lease_info["assigned_ip"] = None
    client_lease_info["preferred_until"] = 0
    client_lease_info["valid_until"] = 0
    client_lease_info["na_state"] = "INIT"

    client_lease_info["client_iapd_iaid"] = None
    client_lease_info["assigned_prefix"] = None
    client_lease_info["assigned_prefix_length"] = None
    client_lease_info["prefix_preferred_until"] = 0
    client_lease_info["prefix_valid_until"] = 0
    client_lease_info["pd_state"] = "INIT"


def process_packet(data, client_addr_info):
    client_ip, client_port = client_addr_info[0], client_addr_info[1]
    client_lease_info["client_link_local"] = client_ip
    client_lease_info["client_source_port"] = client_port

    msg_type_val = data[0]
    parsed_pkt = None

    if msg_type_val == 1: parsed_pkt = DHCP6_Solicit(_pkt=data)
    elif msg_type_val == 3: parsed_pkt = DHCP6_Request(_pkt=data)
    elif msg_type_val == 5: parsed_pkt = DHCP6_Renew(_pkt=data)
    elif msg_type_val == 6: parsed_pkt = DHCP6_Rebind(_pkt=data)
    elif msg_type_val == 8: parsed_pkt = DHCP6_Release(_pkt=data)
    else:
        print_state(f"Received unknown or unhandled DHCPv6 message type: {msg_type_val} from [{client_ip}]:{client_port}")
        return

    if not parsed_pkt:
        print_state("Failed to parse known DHCPv6 message type.")
        return

    print_state(f"Received {parsed_pkt.__class__.__name__} (TrID: {parsed_pkt.trid:#x}) from [{client_ip}]:{client_port}")

    if isinstance(parsed_pkt, DHCP6_Solicit):
        current_duid = parsed_pkt.getlayer(DHCP6OptClientId).duid if DHCP6OptClientId in parsed_pkt else None
        if client_lease_info["client_duid"] and client_lease_info["client_duid"] != current_duid:
            print_state(f"New client DUID {current_duid} received. Resetting previous lease for {client_lease_info['client_duid']}.")
            reset_client_lease()
        elif client_lease_info["na_state"] not in ["INIT", "SELECTING"] or client_lease_info["pd_state"] not in ["INIT", "SELECTING_PD"]:
            print_state(f"Warning: Received SOLICIT in unexpected NA_State '{client_lease_info['na_state']}' or PD_State '{client_lease_info['pd_state']}'. Processing as new.")
            if client_lease_info["client_duid"] == current_duid :
                 reset_client_lease()


        if DHCP6OptClientId not in parsed_pkt:
            print_state("SOLICIT without Client ID. Ignoring.")
            return
        client_lease_info["client_duid"] = parsed_pkt[DHCP6OptClientId].duid
        client_lease_info["transaction_id"] = parsed_pkt.trid

        adv_pkt_payload = DHCP6OptClientId(duid=client_lease_info["client_duid"]) / SERVER_ID_OPT

        ia_na_sol = parsed_pkt.getlayer(DHCP6OptIA_NA)
        if ia_na_sol:
            client_lease_info["client_iana_iaid"] = ia_na_sol.iaid
            ia_na_adv = build_ia_na_option(
                client_lease_info["client_iana_iaid"], OFFERED_IPV6_ADDRESS,
                PREFERRED_LIFETIME, VALID_LIFETIME
            )
            adv_pkt_payload /= ia_na_adv
            client_lease_info["na_state"] = "SELECTING"
        else:
            print_state("SOLICIT without IA_NA. Not offering address.")


        ia_pd_sol = parsed_pkt.getlayer(DHCP6OptIA_PD)
        if ia_pd_sol:
            client_lease_info["client_iapd_iaid"] = ia_pd_sol.iaid
            ia_pd_adv = build_ia_pd_option(
                client_lease_info["client_iapd_iaid"], OFFERED_IPV6_PREFIX, OFFERED_PREFIX_LENGTH,
                PREFIX_PREFERRED_LIFETIME, PREFIX_VALID_LIFETIME
            )
            adv_pkt_payload /= ia_pd_adv
            client_lease_info["pd_state"] = "SELECTING_PD"
        else:
            print_state("SOLICIT without IA_PD. Not offering prefix.")

        if client_lease_info["na_state"] == "SELECTING" or client_lease_info["pd_state"] == "SELECTING_PD":
            adv_pkt = DHCP6_Advertise(trid=parsed_pkt.trid) / adv_pkt_payload / DHCP6OptPref(prefval=PREFERENCE_VALUE)
            send_dhcpv6_response(client_ip, client_port, adv_pkt)
        else:
            print_state("SOLICIT did not request any recognized IAs (IA_NA or IA_PD). Ignoring.")


    elif isinstance(parsed_pkt, DHCP6_Request):
        if DHCP6OptClientId not in parsed_pkt or parsed_pkt[DHCP6OptClientId].duid != client_lease_info["client_duid"]:
            print_state("REQUEST with mismatched Client ID. Ignoring.")
            return

        client_lease_info["transaction_id"] = parsed_pkt.trid
        now = int(time.time())
        reply_pkt_payload = DHCP6OptClientId(duid=client_lease_info["client_duid"]) / SERVER_ID_OPT
        processed_request = False

        ia_na_req = parsed_pkt.getlayer(DHCP6OptIA_NA)
        if ia_na_req and client_lease_info["na_state"] == "SELECTING" and ia_na_req.iaid == client_lease_info["client_iana_iaid"]:
            client_lease_info["assigned_ip"] = OFFERED_IPV6_ADDRESS
            client_lease_info["preferred_until"] = now + PREFERRED_LIFETIME
            client_lease_info["valid_until"] = now + VALID_LIFETIME
            ia_na_rep = build_ia_na_option(
                client_lease_info["client_iana_iaid"], client_lease_info["assigned_ip"],
                PREFERRED_LIFETIME, VALID_LIFETIME
            )
            reply_pkt_payload /= ia_na_rep
            client_lease_info["na_state"] = "BOUND"
            processed_request = True
        elif ia_na_req:
             print_state(f"REQUEST for IA_NA iaid {ia_na_req.iaid} in unexpected state ({client_lease_info['na_state']}) or mismatched IAID. Ignoring IA_NA.")
             reply_pkt_payload /= build_ia_na_option(ia_na_req.iaid, None, 0, 0, status_code=3, status_message="NoBinding")


        ia_pd_req = parsed_pkt.getlayer(DHCP6OptIA_PD)
        if ia_pd_req and client_lease_info["pd_state"] == "SELECTING_PD" and ia_pd_req.iaid == client_lease_info["client_iapd_iaid"]:
            client_lease_info["assigned_prefix"] = OFFERED_IPV6_PREFIX
            client_lease_info["assigned_prefix_length"] = OFFERED_PREFIX_LENGTH
            client_lease_info["prefix_preferred_until"] = now + PREFIX_PREFERRED_LIFETIME
            client_lease_info["prefix_valid_until"] = now + PREFIX_VALID_LIFETIME
            ia_pd_rep = build_ia_pd_option(
                client_lease_info["client_iapd_iaid"], client_lease_info["assigned_prefix"], client_lease_info["assigned_prefix_length"],
                PREFIX_PREFERRED_LIFETIME, PREFIX_VALID_LIFETIME
            )
            reply_pkt_payload /= ia_pd_rep
            client_lease_info["pd_state"] = "BOUND_PD"
            processed_request = True
        elif ia_pd_req:
            print_state(f"REQUEST for IA_PD iaid {ia_pd_req.iaid} in unexpected state ({client_lease_info['pd_state']}) or mismatched IAID. Ignoring IA_PD.")
            reply_pkt_payload /= build_ia_pd_option(ia_pd_req.iaid, None, None, 0, 0, status_code=3, status_message="NoBinding")


        if processed_request:
            reply_pkt = DHCP6_Reply(trid=parsed_pkt.trid) / reply_pkt_payload
            send_dhcpv6_response(client_ip, client_port, reply_pkt)
        else:
            if client_lease_info["na_state"] != "SELECTING" and client_lease_info["pd_state"] != "SELECTING_PD" and (ia_na_req or ia_pd_req):
                 reply_pkt = DHCP6_Reply(trid=parsed_pkt.trid) / reply_pkt_payload
                 send_dhcpv6_response(client_ip, client_port, reply_pkt)
            else:
                 print_state("REQUEST contained no valid IAs matching offered ones or server not in selecting state. Ignoring.")


    elif isinstance(parsed_pkt, DHCP6_Renew):
        if DHCP6OptClientId not in parsed_pkt or parsed_pkt[DHCP6OptClientId].duid != client_lease_info["client_duid"]:
            print_state("RENEW with mismatched Client ID. Ignoring.")
            return
        if DHCP6OptServerId not in parsed_pkt or parsed_pkt[DHCP6OptServerId].duid != SERVER_DUID:
            print_state("RENEW not for this server (Server ID mismatch). Ignoring.")
            return

        client_lease_info["transaction_id"] = parsed_pkt.trid
        now = int(time.time())
        reply_pkt_payload = DHCP6OptClientId(duid=client_lease_info["client_duid"]) / SERVER_ID_OPT
        processed_renew = False

        ia_na_renew = parsed_pkt.getlayer(DHCP6OptIA_NA)
        if ia_na_renew and client_lease_info["na_state"] == "BOUND" and \
           ia_na_renew.iaid == client_lease_info["client_iana_iaid"] and \
           client_lease_info["assigned_ip"] is not None:
            client_lease_info["preferred_until"] = now + PREFERRED_LIFETIME
            client_lease_info["valid_until"] = now + VALID_LIFETIME
            ia_na_rep = build_ia_na_option(
                client_lease_info["client_iana_iaid"], client_lease_info["assigned_ip"],
                PREFERRED_LIFETIME, VALID_LIFETIME
            )
            reply_pkt_payload /= ia_na_rep
            processed_renew = True
        elif ia_na_renew:
            reply_pkt_payload /= build_ia_na_option(ia_na_renew.iaid, None, 0, 0, status_code=3, status_message="NoBinding")

        ia_pd_renew = parsed_pkt.getlayer(DHCP6OptIA_PD)
        if ia_pd_renew and client_lease_info["pd_state"] == "BOUND_PD" and \
           ia_pd_renew.iaid == client_lease_info["client_iapd_iaid"] and \
           client_lease_info["assigned_prefix"] is not None:
            client_lease_info["prefix_preferred_until"] = now + PREFIX_PREFERRED_LIFETIME
            client_lease_info["prefix_valid_until"] = now + PREFIX_VALID_LIFETIME
            ia_pd_rep = build_ia_pd_option(
                client_lease_info["client_iapd_iaid"], client_lease_info["assigned_prefix"], client_lease_info["assigned_prefix_length"],
                PREFIX_PREFERRED_LIFETIME, PREFIX_VALID_LIFETIME
            )
            reply_pkt_payload /= ia_pd_rep
            processed_renew = True
        elif ia_pd_renew:
            reply_pkt_payload /= build_ia_pd_option(ia_pd_renew.iaid, None, None, 0, 0, status_code=3, status_message="NoBinding")


        if processed_renew or ia_na_renew or ia_pd_renew :
            reply_pkt = DHCP6_Reply(trid=parsed_pkt.trid) / reply_pkt_payload
            send_dhcpv6_response(client_ip, client_port, reply_pkt)
        else:
            print_state("RENEW did not contain any IA_NA or IA_PD to process. Ignoring.")


    elif isinstance(parsed_pkt, DHCP6_Rebind):
        if DHCP6OptClientId not in parsed_pkt or parsed_pkt[DHCP6OptClientId].duid != client_lease_info["client_duid"]:
            print_state("REBIND with mismatched Client ID. Ignoring.")
            return

        client_lease_info["transaction_id"] = parsed_pkt.trid
        now = int(time.time())
        reply_pkt_payload = DHCP6OptClientId(duid=client_lease_info["client_duid"]) / SERVER_ID_OPT
        processed_rebind = False

        ia_na_rebind = parsed_pkt.getlayer(DHCP6OptIA_NA)
        if ia_na_rebind and client_lease_info["na_state"] == "BOUND" and \
           ia_na_rebind.iaid == client_lease_info["client_iana_iaid"] and \
           client_lease_info["assigned_ip"] is not None:
            client_lease_info["preferred_until"] = now + PREFERRED_LIFETIME
            client_lease_info["valid_until"] = now + VALID_LIFETIME
            ia_na_rep = build_ia_na_option(
                client_lease_info["client_iana_iaid"], client_lease_info["assigned_ip"],
                PREFERRED_LIFETIME, VALID_LIFETIME
            )
            reply_pkt_payload /= ia_na_rep
            processed_rebind = True
        elif ia_na_rebind :
             reply_pkt_payload /= build_ia_na_option(ia_na_rebind.iaid, None, 0, 0, status_code=3, status_message="NoBinding")


        ia_pd_rebind = parsed_pkt.getlayer(DHCP6OptIA_PD)
        if ia_pd_rebind and client_lease_info["pd_state"] == "BOUND_PD" and \
           ia_pd_rebind.iaid == client_lease_info["client_iapd_iaid"] and \
           client_lease_info["assigned_prefix"] is not None:
            client_lease_info["prefix_preferred_until"] = now + PREFIX_PREFERRED_LIFETIME
            client_lease_info["prefix_valid_until"] = now + PREFIX_VALID_LIFETIME
            ia_pd_rep = build_ia_pd_option(
                client_lease_info["client_iapd_iaid"], client_lease_info["assigned_prefix"], client_lease_info["assigned_prefix_length"],
                PREFIX_PREFERRED_LIFETIME, PREFIX_VALID_LIFETIME
            )
            reply_pkt_payload /= ia_pd_rep
            processed_rebind = True
        elif ia_pd_rebind :
            reply_pkt_payload /= build_ia_pd_option(ia_pd_rebind.iaid, None, None, 0, 0, status_code=3, status_message="NoBinding")

        if processed_rebind or ia_na_rebind or ia_pd_rebind:
            reply_pkt = DHCP6_Reply(trid=parsed_pkt.trid) / reply_pkt_payload
            send_dhcpv6_response(client_ip, client_port, reply_pkt)
        else:
            print_state("REBIND: Server not authoritative for any requested IAs or no IAs to process. Ignoring.")


    elif isinstance(parsed_pkt, DHCP6_Release):
        if DHCP6OptClientId not in parsed_pkt or parsed_pkt[DHCP6OptClientId].duid != client_lease_info["client_duid"]:
            print_state("RELEASE with mismatched Client ID. Ignoring.")
            return
        if DHCP6OptServerId not in parsed_pkt or parsed_pkt[DHCP6OptServerId].duid != SERVER_DUID:
            print_state("RELEASE not for this server (Server ID mismatch). Ignoring.")
            return

        client_lease_info["transaction_id"] = parsed_pkt.trid
        reply_pkt_payload = DHCP6OptClientId(duid=client_lease_info["client_duid"]) / SERVER_ID_OPT
        released_something = False

        ia_na_release = parsed_pkt.getlayer(DHCP6OptIA_NA)
        if ia_na_release and client_lease_info["na_state"] == "BOUND" and \
           ia_na_release.iaid == client_lease_info["client_iana_iaid"]:
            print_state(f"Releasing IP {client_lease_info['assigned_ip']} for IAID_NA {ia_na_release.iaid}")
            client_lease_info["assigned_ip"] = None
            client_lease_info["preferred_until"] = 0
            client_lease_info["valid_until"] = 0
            client_lease_info["na_state"] = "INIT"
            reply_pkt_payload /= build_ia_na_option(ia_na_release.iaid, None, 0, 0, status_code=0, status_message="Success")
            released_something = True
        elif ia_na_release:
            reply_pkt_payload /= build_ia_na_option(ia_na_release.iaid, None, 0, 0, status_code=3, status_message="NoBinding")


        ia_pd_release = parsed_pkt.getlayer(DHCP6OptIA_PD)
        if ia_pd_release and client_lease_info["pd_state"] == "BOUND_PD" and \
           ia_pd_release.iaid == client_lease_info["client_iapd_iaid"]:
            print_state(f"Releasing Prefix {client_lease_info['assigned_prefix']}/{client_lease_info['assigned_prefix_length']} for IAID_PD {ia_pd_release.iaid}")
            client_lease_info["assigned_prefix"] = None
            client_lease_info["assigned_prefix_length"] = None
            client_lease_info["prefix_preferred_until"] = 0
            client_lease_info["prefix_valid_until"] = 0
            client_lease_info["pd_state"] = "INIT"
            reply_pkt_payload /= build_ia_pd_option(ia_pd_release.iaid, None, None, 0, 0, status_code=0, status_message="Success")
            released_something = True
        elif ia_pd_release:
            reply_pkt_payload /= build_ia_pd_option(ia_pd_release.iaid, None, None, 0, 0, status_code=3, status_message="NoBinding")

        if not ia_na_release and not ia_pd_release:
             reply_pkt_payload /= DHCP6OptStatusCode(statuscode=0, statusmsg="Success")
        elif not released_something and (ia_na_release or ia_pd_release):
             pass

        reply_pkt = DHCP6_Reply(trid=parsed_pkt.trid) / reply_pkt_payload
        send_dhcpv6_response(client_ip, client_port, reply_pkt)

        if client_lease_info["na_state"] == "INIT" and client_lease_info["pd_state"] == "INIT":
            print_state("All leases for client released.")


def get_link_local_addr(interface):
    import subprocess
    result = subprocess.run(['ip', '-6', 'addr', 'show', 'dev', interface], capture_output=True, text=True)
    for line in result.stdout.splitlines():
        line = line.strip()
        if line.startswith('inet6') and 'scope link' in line:
            addr = line.split()[1].split('/')[0]
            return addr
    raise RuntimeError(f"No link-local IPv6 address found for interface {interface}")

def main():
    parser = argparse.ArgumentParser(description="Mock DHCPv6 server with Prefix Delegation")
    parser.add_argument('-i', '--interface', required=True, help='Interface to bind to (e.g., veth-dhcp-srv)')
    args = parser.parse_args()
    interface = args.interface

    sock = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        sock.setsockopt(socket.SOL_SOCKET, 25, interface.encode() + b'\0')
    except OSError as e:
        print_state(f"Warning: Failed to SO_BINDTODEVICE ({e}). This might not work as expected without root/CAP_NET_RAW.")

    import struct
    try:
        group = socket.inet_pton(socket.AF_INET6, 'ff02::1:2')
        ifindex = socket.if_nametoindex(interface)
        mreq = group + struct.pack('@I', ifindex)
        sock.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_JOIN_GROUP, mreq)
    except OSError as e:
        print_state(f"Failed to join multicast group or get ifindex for {interface}: {e}")
        sock.close()
        return
    except Exception as e:
        print_state(f"An error occurred during multicast setup: {e}")
        sock.close()
        return

    try:
        sock.bind(('::', SERVER_PORT))
        print_state(f"Mock DHCPv6 server listening on [::]%{interface}:{SERVER_PORT} (SO_BINDTODEVICE might be active)")
    except Exception as e:
        print_state(f"Failed to bind socket: {e}. Ensure port is not in use and you have permissions if needed.")
        sock.close()
        return

    try:
        while True:
            print_state(f"Waiting for DHCPv6 packet...")
            data, client_addr_info = sock.recvfrom(2048)
            process_packet(data, client_addr_info)
    except KeyboardInterrupt:
        print_state("Server shutting down...")
    except Exception as e:
        import traceback
        tb_str = traceback.format_exc()
        print_state(f"An critical error occurred in the main loop: {e}\nTraceback:\n{tb_str}")
    finally:
        sock.close()
        print_state("Server socket closed.")

if __name__ == "__main__":
    main()

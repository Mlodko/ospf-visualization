FROM debian:stable-slim

RUN apt-get update && \
    apt-get install -y frr frr-snmp snmp snmpd iproute2

# Copy your configs (adjust as needed)
COPY frr.conf /etc/frr/frr.conf
COPY snmpd.conf /etc/snmp/snmpd.conf

# Start both daemons in the foreground for debugging
CMD service snmpd start && /usr/lib/frr/frrinit.sh start && tail -f /dev/null

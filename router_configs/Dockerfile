FROM debian:bookworm

# Install dependencies
RUN apt-get update && \
    apt-get install -y gnupg2 snmpd curl lsb-release

# Add FRRouting repository
RUN curl -s https://deb.frrouting.org/frr/keys.asc | apt-key add - && \
    echo "deb https://deb.frrouting.org/frr $(lsb_release -cs) frr-stable" > /etc/apt/sources.list.d/frr.list

# Install FRR with SNMP support
RUN apt-get update && \
    apt-get install -y frr frr-snmp

# Enable daemons
COPY ./daemons /etc/frr/daemons

# Entrypoint: start snmpd and FRR
CMD service snmpd start && /usr/lib/frr/frrinit.sh start && tail -f /var/log/frr/ospfd.log

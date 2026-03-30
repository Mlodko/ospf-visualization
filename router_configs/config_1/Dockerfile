FROM debian:bookworm

# Install dependencies
RUN apt-get update && \
    apt-get install -y gnupg2 snmpd curl lsb-release inetutils-ping inetutils-traceroute iptables

# Add FRRouting repository
RUN curl -s https://deb.frrouting.org/frr/keys.asc | apt-key add - && \
    echo "deb https://deb.frrouting.org/frr $(lsb_release -cs) frr-stable" > /etc/apt/sources.list.d/frr.list

# Install FRR with SNMP support
RUN apt-get update && \
    apt-get install -y frr frr-snmp openssh-server
    
RUN mkdir /var/run/sshd

RUN useradd -m client && echo 'client:password' | chpasswd
RUN usermod -aG frrvty client && usermod -aG frr client

# Enable daemons
COPY ./daemons /etc/frr/daemons

# Entrypoint: start snmpd and FRR
CMD service ssh start && service snmpd start && /usr/lib/frr/frrinit.sh start && tail -F /var/log/frr/ospfd.log /var/log/frr/isisd.log

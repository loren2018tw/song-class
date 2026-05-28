export interface PeerConnectionOptions {
  onIceCandidate?: (candidate: RTCIceCandidate) => void;
  onDataChannel?: (channel: RTCDataChannel) => void;
  onConnectionStateChange?: (state: RTCPeerConnectionState) => void;
}

export function createPeerConnection(options: PeerConnectionOptions = {}) {
  const connection = new RTCPeerConnection({
    iceServers: [{ urls: ["stun:stun.l.google.com:19302"] }],
    iceCandidatePoolSize: 10,
  });

  connection.onicecandidate = (event) => {
    if (event.candidate && options.onIceCandidate) {
      options.onIceCandidate(event.candidate);
    }
  };

  connection.ondatachannel = (event) => {
    if (options.onDataChannel) {
      options.onDataChannel(event.channel);
    }
  };

  connection.onconnectionstatechange = () => {
    if (options.onConnectionStateChange) {
      options.onConnectionStateChange(connection.connectionState);
    }
  };

  return connection;
}

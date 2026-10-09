self.onmessage = ({ data }) => postMessage({ source: 'resource-worker', message: data })

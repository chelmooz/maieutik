import { useEffect } from 'react';
import { useFiles } from '../store/useFiles';

export default function FileList() {
  const { files, loading, fetch } = useFiles();
  useEffect(() => { fetch(); }, [fetch]);
  if (loading) return <div>Chargement...</div>;
  return (
    <div>
      <h1>Fichiers</h1>
      <ul>
        {files.map(f => <li key={f}>{f}</li>)}
      </ul>
    </div>
  );
}

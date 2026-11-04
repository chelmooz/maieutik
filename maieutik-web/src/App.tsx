import { Routes, Route, Link } from 'react-router-dom'
import FileList from './pages/FileList'

export default function App() {
  return (
    <div>
      <nav>
        <Link to="/">Files</Link>
      </nav>
      <Routes>
        <Route path="/" element={<FileList />} />
      </Routes>
    </div>
  )
}

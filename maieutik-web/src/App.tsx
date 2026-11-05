import { Routes, Route, Link } from 'react-router-dom'
import FileList from './pages/FileList'
import { Chat } from './pages/Chat'

export default function App() {
  return (
    <div>
      <nav>
        <Link to="/">Files</Link>
        <Link to="/chat" style={{marginLeft: '1rem'}}>Chat</Link>
      </nav>
      <Routes>
        <Route path="/" element={<FileList />} />
        <Route path="/chat" element={<Chat />} />
      </Routes>
    </div>
  )
}

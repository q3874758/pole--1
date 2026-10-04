Set ws = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")
currentDir = fso.GetParentFolderName(WScript.ScriptFullName)
ws.CurrentDirectory = currentDir

If fso.FolderExists(currentDir & "\.git") Then
    ws.Run "cmd /c git pull --ff-only origin main", 0, True
End If

If fso.FileExists(currentDir & "\pole.exe") Then
    ws.Run """" & currentDir & "\pole.exe""", 0, False
ElseIf fso.FileExists(currentDir & "\target\release\pole.exe") Then
    ws.Run """" & currentDir & "\target\release\pole.exe""", 0, False
ElseIf fso.FileExists(currentDir & "\target\debug\pole.exe") Then
    ws.Run """" & currentDir & "\target\debug\pole.exe""", 0, False
End If
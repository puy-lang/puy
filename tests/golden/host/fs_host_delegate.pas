// puy-test: test-fs, clock=100
program FsHostDelegate;

// Host access remains explicit when an override is installed. The host adapter can itself be
// installed without re-entering the Pascal dispatch, and native extension calls see the host.

uses
  System.SysUtils, System.IOUtils, System.Classes, Puy.Paths, Puy.Testing, System.IOUtils.Native;

begin
  var Host := TFileSystem.Host;
  Host.CreateDirectory('delegated', False);
  Host.CreateDirectory('delegated/sub/deep', True);
  var H := Host.OpenFile('delegated/data', TFileOpenMode.omCreate);
  H.Write([11, 22, 33], 1, 2);
  H.Close;

  TFileSystem.Install(TMemoryFileSystem.Create);
  TDirectory.CreateDirectory('delegated');
  TFile.WriteAllBytes('delegated/data', [99]);
  var MemBytes := TFile.ReadAllBytes('delegated/data');
  WriteLn('installed data: ', MemBytes[0]);
  WriteLn('host info: ', Host.GetInfo('delegated/data', True).Size, ' ',
    System.IOUtils.Native.FileInfo('delegated/data', True).Size);
  WriteLn('host canonical: ', Host.Canonicalize('delegated/./data'));
  for var Entry in Host.ListDirectory('delegated') do
    WriteLn('host entry: ', Entry.Name, ' ', Entry.Kind);

  TFileSystem.Install(Host);
  Host.CopyFile('delegated/data', 'delegated/copied');
  RenameFile('delegated/copied', 'delegated/moved');
  var HostBytes := TFile.ReadAllBytes('delegated/moved');
  WriteLn('host copy: ', HostBytes[0], ' ', HostBytes[1]);
  Host.SetTime('delegated/moved', TFileTimeKind.ftLastWrite, 42);
  Host.SetReadOnly('delegated/moved', True);
  var Info := Host.GetInfo('delegated/moved', True);
  WriteLn('host attributes: ', Info.Modified, ' ', Info.ReadOnly);
  Host.SetReadOnly('delegated/moved', False);
  TDirectory.Delete('delegated/sub', True);
  Host.RemoveFile('delegated/moved');
  WriteLn('host removed: ', not DirectoryExists('delegated/sub'), ' ', not FileExists('delegated/moved'));

  TFileSystem.Uninstall;
  var Stream := TFileStream.Create('delegated/data', fmOpenRead);
  WriteLn('default handle: ', Stream.Handle <> nil);
end.

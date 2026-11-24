program FileWorkspace;

uses
  System.SysUtils, System.Classes, System.IOUtils, System.Diagnostics,
  Puy.OS, Puy.Paths, Puy.Platform, Puy.Testing;

// Use the regular file APIs with an in-memory workspace and a fixed starting time.
procedure WriteReports;
begin
  var Root: TFilePath := 'reports';
  Root.CreateDirectory;
  var Draft := Root / 'draft.txt';
  Draft.WriteAllText('status=pending', TEncoding.UTF8);

  var Timer := TStopwatch.StartNew;
  Sleep(1500); // The virtual clock advances without waiting.

  var Published := Root / 'published.txt';
  var Stream := TFileStream.Create(Published, fmCreate);
  try
    var Bytes := TEncoding.UTF8.GetBytes('status=published');
    Stream.WriteBuffer(Bytes, 0, Length(Bytes));
  finally
    Stream.Close;
  end;

  for var Report in Root.Files('*.txt') do
    WriteLn(Report.Name, ': ', Report.ReadAllText(TEncoding.UTF8));
  WriteLn('Published at: ', TFile.GetLastWriteInstant(Published).ToString);
  WriteLn('Elapsed milliseconds: ', Timer.ElapsedMilliseconds);
end;

begin
  WriteLn('Host filesystem: ', TCapabilities.Status(TCapability.FileSystem));
  if TCapabilities.OS then
    WriteLn('Logical processors: ', GetProcessorCount);

  TTestClock.Freeze(TInstant.Parse('2026-06-01T09:00:00Z'));
  TFileSystem.Install(TMemoryFileSystem.Create('/workspace'));
  try
    WriteReports;
  finally
    TFileSystem.Uninstall;
  end;
end.

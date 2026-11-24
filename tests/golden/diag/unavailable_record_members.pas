program UnavailableRecordMembers;

// A member a unit lists as not provided is E3022 on a record type too (System.IOUtils lists the
// times of TDirectory, which TFile's take a directory for; spec/stdlib/System.IOUtils/README.md).
// Another missing member of the record stays E0700.

uses
  System.IOUtils;

begin
  WriteLn(TDirectory.GetLastWriteTime('a'));
  TDirectory.SetCreationTimeUtc('a', 0);
  TDirectory.NoSuchMember;
end.

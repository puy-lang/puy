// A helper unit whose finalization raises: every test that uses it ends as ERROR.
unit Closer;

interface

procedure Touch;

implementation

uses
  System.SysUtils;

procedure Touch;
begin
end;

finalization
  raise EInvalidOperation.Create('closing failed');
end.

unit Users;

interface

procedure Run;

implementation

uses Holders;

procedure Run;
begin
  THolder<Integer>.Keep('ints', 7);
end;

initialization
  THolder<string>.Keep('strings in Users', 'a');
finalization
  WriteLn('fini Users ', THolder<Integer>.Tag.Name, ' ', Length(THolder<string>.Values));
end.

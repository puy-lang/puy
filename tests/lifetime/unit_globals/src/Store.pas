unit Store;

interface

type
  TEntry = record
    Key: string;
    Tags: array of string;
  end;

var
  Items: array of string;
  Last: TEntry;

procedure Add(const S: string);

implementation

procedure Add(const S: string);
begin
  SetLength(Items, Length(Items) + 1);
  Items[High(Items)] := S;
  Last.Key := S;
  Last.Tags := Items;
end;

initialization
  Add('first');
finalization
  WriteLn('fini Store ', Length(Items), ' ', Last.Key, ' ', Length(Last.Tags));
end.

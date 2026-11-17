program StdlibDictionaryViews;

// Keys and Values (collections.md 7.3) show the dictionary as it is now, in insertion order.
// The dictionary owns them; one freed by mistake is made again the next time it is read, and the
// dictionary does not free it a second time.

uses System.SysUtils, System.Generics.Collections;

var
  D: TDictionary<Integer, string>;
  Keys: TEnumerable<Integer>;
  K: Integer;
  S: string;

procedure ShowAll(const Title: string; const Items: TEnumerable<Integer>);
var
  N: Integer;
begin
  Write(Title, ':');
  for N in Items do
    Write(' ', N);
  WriteLn;
end;

begin
  D := TDictionary<Integer, string>.Create;
  D.Add(3, 'three');
  D.Add(1, 'one');
  D.Add(2, 'two');
  Keys := D.Keys;
  Write('keys:');
  for K in D.Keys do
    Write(' ', K);
  WriteLn;
  Write('values:');
  for S in D.Values do
    Write(' ', S);
  WriteLn;
  WriteLn('counts ', D.Keys.Count, ' ', D.Values.Count, ' ', Length(D.Values.ToArray));

  // The same view, which follows the dictionary
  D.Remove(1);
  D.Add(4, 'four');
  WriteLn('same view: ', Keys = TEnumerable<Integer>(D.Keys));
  ShowAll('keys now', Keys);

  // Freed by mistake: no exception, and reading Keys again makes a new view
  D.Keys.Free;
  D.Values.Free;
  ShowAll('keys again', D.Keys);
  WriteLn('values again: ', Length(D.Values.ToArray));
  D.Free;
  WriteLn('done');
end.

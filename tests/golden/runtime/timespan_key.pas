program TimeSpanKey;

// TTimeSpan as a dictionary key and a list element: the key equality and hash and the list's
// equality use its methods (GetHashCode, Equal), whose bodies are analyzed only because the type
// is a type argument (units.md 6.1). Nothing else of the program uses them.

uses
  System.Generics.Collections;

var
  D: TDictionary<TTimeSpan, string>;
  L: TList<TTimeSpan>;
begin
  D := TDictionary<TTimeSpan, string>.Create;
  D.Add(TTimeSpan.FromSeconds(90), 'ninety');
  D.Add(TTimeSpan.FromMinutes(2), 'two minutes');
  WriteLn(D[TTimeSpan.FromSeconds(120)]);
  WriteLn(D.ContainsKey(TTimeSpan.FromSeconds(90)), ' ', D.ContainsKey(TTimeSpan.FromSeconds(91)));
  D.Free;
  L := TList<TTimeSpan>.Create;
  L.Add(TTimeSpan.FromHours(1));
  L.Add(TTimeSpan.FromMinutes(30));
  WriteLn(L.IndexOf(TTimeSpan.FromSeconds(1800)), ' ', L.Contains(TTimeSpan.FromSeconds(1)));
  L.Free;
end.

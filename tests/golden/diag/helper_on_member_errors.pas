// A method no helper has, called on a field of a record, is reported on the method's name for the
// field's type (objects.md 10.4), not as a missing field of the record; a field the record does not
// have is still reported on the field.
program HelperOnMemberErrors;

type
  TItem = record
    Name: string;
    Count: Integer;
  end;

var
  X: TItem;

begin
  WriteLn(X.Name.Missing('a'));
  WriteLn(X.Count.Missing(1));
  WriteLn(X.Absent.StartsWith('a'));
end.

; Elements
(svg_root_element) @entry.around
(element) @entry.around
(self_closing_tag) @entry.around

; Attributes
(attribute) @parameter.around
(attribute
  (_ value: (_) @parameter.inside))
(animate_motion_coordinate_attribute) @parameter.around
(animate_motion_coordinate_attribute value: (_) @parameter.inside)
(animate_motion_values_attribute) @parameter.around
(animate_motion_values_attribute value: (_) @parameter.inside)

; Comments
(comment) @comment.around
(comment text: (comment_text) @comment.inside)
